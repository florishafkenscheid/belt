//! Running one Factorio process: stream its stdout line by line, keep a bounded tail of its
//! output for failure reports, let the caller write to its stdin, and stop it on Ctrl-C.
//!
//! Nothing here polls. A waiter thread learns when Factorio exits and then kills anything it
//! started (see `RUN_MARKER`), which also closes the last copies of its output pipes; the
//! readers then reach end of file.

use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::workspace::{RUN_MARKER, RunDir};
use crate::failure::{FailureReason, RunFailure};

/// How many lines of Factorio's output a failure report keeps.
const TAIL_LINES: usize = 30;

/// The last lines of Factorio's output (stdout and stderr), never more than its capacity.
#[derive(Debug)]
pub struct Tail {
    lines: VecDeque<String>,
    capacity: usize,
}

impl Tail {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, line: &str) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line.to_string());
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

/// Factorio's stdin, for console commands such as `/quit`.
pub struct Stdin(ChildStdin);

impl Stdin {
    pub fn send(&mut self, line: &str) -> std::io::Result<()> {
        writeln!(self.0, "{line}")?;
        self.0.flush()
    }
}

/// How a Factorio process ended.
#[derive(Debug)]
pub struct Finished {
    /// `None` when killed by a signal.
    pub code: Option<i32>,
    pub tail: Vec<String>,
    /// Ctrl-C arrived while it ran.
    pub interrupted: bool,
}

struct Shared {
    /// PIDs of the running Factorios. Each is still our unreaped child while listed, so its PID
    /// can't have been reused when Ctrl-C kills it.
    running: Mutex<HashSet<u32>>,
    interrupted: AtomicBool,
}

static SHARED: OnceLock<Result<Shared, String>> = OnceLock::new();

/// Runs Factorio processes, one per call. On Ctrl-C every running Factorio is killed; in a
/// terminal Factorio also receives the Ctrl-C itself.
/// Whether Factorio's launches and output are also shown (`--verbose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Echo {
    Quiet,
    Verbose,
}

pub struct Supervisor {
    shared: &'static Shared,
    echo: Echo,
}

impl Supervisor {
    pub fn new(echo: Echo) -> Result<Self, String> {
        let shared = SHARED.get_or_init(|| {
            ctrlc::set_handler(|| {
                let Some(Ok(shared)) = SHARED.get() else {
                    return;
                };
                shared.interrupted.store(true, Ordering::SeqCst);
                for pid in shared.running.lock().expect("lock").iter() {
                    kill(*pid);
                }
            })
            .map_err(|error| format!("cannot install the Ctrl-C handler: {error}"))?;
            Ok(Shared {
                running: Mutex::new(HashSet::new()),
                interrupted: AtomicBool::new(false),
            })
        });
        match shared {
            Ok(shared) => Ok(Self { shared, echo }),
            Err(error) => Err(error.clone()),
        }
    }

    pub fn interrupted(&self) -> bool {
        self.shared.interrupted.load(Ordering::SeqCst)
    }

    /// Runs `command` to its end, handing each stdout line to `on_line`.
    pub fn run(
        &self,
        mut command: Command,
        run: &RunDir,
        mut on_line: impl FnMut(&str, &mut Stdin),
    ) -> Result<Finished, RunFailure> {
        if self.interrupted() {
            return Err(RunFailure::new(FailureReason::Interrupted));
        }
        command
            .env(RUN_MARKER, run.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.echo == Echo::Verbose {
            eprintln!("running {command:?}");
        }
        let mut child = command.spawn().map_err(|error| {
            RunFailure::new(FailureReason::Setup(format!(
                "cannot start Factorio: {error}"
            )))
        })?;
        let pid = child.id();
        if let Err(error) = run.record_process(pid) {
            eprintln!("cannot record Factorio's process for crash cleanup: {error}");
        }
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let mut stdin = Stdin(child.stdin.take().expect("piped"));
        self.shared.running.lock().expect("lock").insert(pid);
        let tail = Arc::new(Mutex::new(Tail::new(TAIL_LINES)));
        let stderr_tail = Arc::clone(&tail);
        let status = std::thread::scope(|scope| {
            let waiter = scope.spawn(move || {
                let status = child.wait();
                run.kill_leftovers();
                status
            });
            let stderr_reader = scope.spawn(move || {
                for_each_line(stderr, |line| stderr_tail.lock().expect("lock").push(line));
            });
            for_each_line(stdout, |line| {
                if self.echo == Echo::Verbose {
                    eprintln!("factorio: {line}");
                }
                tail.lock().expect("lock").push(line);
                on_line(line, &mut stdin);
            });
            stderr_reader.join().expect("stderr reader");
            waiter.join().expect("waiter")
        });
        self.shared.running.lock().expect("lock").remove(&pid);
        let status = status.map_err(|error| {
            RunFailure::new(FailureReason::Setup(format!(
                "cannot wait for Factorio: {error}"
            )))
        })?;
        let tail = tail.lock().expect("lock").lines();
        Ok(Finished {
            code: status.code(),
            tail,
            interrupted: self.interrupted(),
        })
    }
}

/// Kills a running Factorio by PID; one that already exited is left alone.
fn kill(pid: u32) {
    let Ok(mut system) = sysinfo::System::new() else {
        return;
    };
    let pid = sysinfo::Pid::from_u32(pid);
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    if let Some(process) = system.process(pid) {
        process.kill();
    }
}

/// Reads lines until EOF; invalid UTF-8 is replaced rather than ending the run.
fn for_each_line(reader: impl Read, mut each: impl FnMut(&str)) {
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) => return,
            Ok(_) => each(String::from_utf8_lossy(&buffer).trim_end_matches(['\r', '\n'])),
            Err(error) => {
                each(&format!(
                    "(belt: reading Factorio's output failed: {error})"
                ));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_factorio_output_is_bounded() {
        let mut tail = Tail::new(2);

        ["one", "two", "three"]
            .into_iter()
            .for_each(|line| tail.push(line));

        assert_eq!(tail.lines(), vec!["two", "three"]);
    }

    #[test]
    fn nothing_factorio_starts_outlives_its_run() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = super::super::workspace::Workspace::open(dir.path()).unwrap();
        let run = workspace.new_run().unwrap();
        let mut command = Command::new("sh");
        // Like a crash dialog: a child that keeps running after its parent has exited.
        command.args(["-c", "sleep 60 & echo $!"]);
        let mut leftover = String::new();

        Supervisor::new(Echo::Quiet)
            .unwrap()
            .run(command, &run, |line, _| leftover = line.to_string())
            .unwrap();

        assert!(gone(leftover.parse().unwrap()), "pid {leftover} still runs");
    }

    /// Whether the process exited, waiting briefly for a killed process to be reaped.
    fn gone(pid: u32) -> bool {
        let pid = sysinfo::Pid::from_u32(pid);
        for _ in 0..50 {
            let mut system = sysinfo::System::new().unwrap();
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
            match system.process(pid) {
                None => return true,
                Some(process) if process.status() == sysinfo::ProcessStatus::Zombie => return true,
                Some(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        false
    }
}
