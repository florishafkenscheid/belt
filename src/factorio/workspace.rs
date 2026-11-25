//! B.E.L.T.'s own data directory: a global lock, the mod cache, and one directory per Factorio
//! run. A run directory records its Factorio's PID and start time, so a later CLI can kill a
//! Factorio that outlived a crashed CLI (a stamping server never exits on its own).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const PROCESS_FILE: &str = "factorio.pid";

/// Every Factorio of a run carries this environment variable, set to the run directory, and
/// anything it starts inherits it (the full build's crash dialog, say). After the run, and in the
/// sweep, processes still carrying it are killed, so nothing a Factorio started outlives its run
/// or holds the run's files.
pub const RUN_MARKER: &str = "BELT_RUN";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("another belt is running (it holds {0}); only one Factorio may run at a time")]
    Locked(PathBuf),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> WorkspaceError + '_ {
    move |source| WorkspaceError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The data directory, locked for this process's lifetime.
#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    _lock: File,
}

impl Workspace {
    /// Takes the lock (failing if another belt holds it), then sweeps run directories left by
    /// a crashed belt: their recorded Factorio is killed if it still runs, and they are removed.
    /// Both are logged.
    pub fn open(root: &Path) -> Result<Self, WorkspaceError> {
        std::fs::create_dir_all(root.join("runs")).map_err(io(root))?;
        let lock_path = root.join("lock");
        let lock = File::create(&lock_path).map_err(io(&lock_path))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(WorkspaceError::Locked(lock_path));
            }
            Err(std::fs::TryLockError::Error(source)) => {
                return Err(WorkspaceError::Io {
                    path: lock_path,
                    source,
                });
            }
        }
        let workspace = Self {
            root: root.to_path_buf(),
            _lock: lock,
        };
        workspace.sweep()?;
        Ok(workspace)
    }

    pub fn mod_cache(&self) -> PathBuf {
        self.root.join("mods")
    }

    /// Mod portal downloads that aren't mods of a run (the belt-sanitizer release).
    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }

    fn runs(&self) -> PathBuf {
        self.root.join("runs")
    }

    fn sweep(&self) -> Result<(), WorkspaceError> {
        let runs = self.runs();
        for entry in std::fs::read_dir(&runs).map_err(io(&runs))? {
            let dir = entry.map_err(io(&runs))?.path();
            if let Some(recorded) = RecordedProcess::read(&dir) {
                recorded.kill_if_alive(&dir);
            }
            kill_marked(&dir);
            std::fs::remove_dir_all(&dir).map_err(io(&dir))?;
            tracing::info!(
                "removed run directory {} left by an earlier belt",
                dir.display()
            );
        }
        Ok(())
    }

    /// A fresh, empty run directory, removed again when dropped.
    pub fn new_run(&self) -> Result<RunDir, WorkspaceError> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = self.runs().join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(io(&path))?;
        Ok(RunDir { path })
    }
}

/// One Factorio run's directory: its config, write-data and process record.
#[derive(Debug)]
pub struct RunDir {
    path: PathBuf,
}

impl RunDir {
    /// Factorio's write-data directory for this run (its log, saves and `player-data.json`).
    pub fn write_data(&self) -> PathBuf {
        self.path.join("data")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Kills whatever the run's Factorio left running (see [`RUN_MARKER`]).
    pub fn kill_leftovers(&self) {
        kill_marked(&self.path);
    }

    /// Records the running Factorio so a later belt can clean up after a crash.
    pub fn record_process(&self, pid: u32) -> Result<(), WorkspaceError> {
        let mut system = System::new().map_err(|error| WorkspaceError::Io {
            path: self.path.clone(),
            source: std::io::Error::other(error.to_string()),
        })?;
        let sys_pid = Pid::from_u32(pid);
        system.refresh_processes(ProcessesToUpdate::Some(&[sys_pid]), true);
        // A process that already exited leaves nothing to clean up, so nothing is recorded.
        let Some(start_time) = system.process(sys_pid).map(|process| process.start_time()) else {
            return Ok(());
        };
        let path = self.path.join(PROCESS_FILE);
        std::fs::write(&path, format!("{pid} {start_time}\n")).map_err(io(&path))
    }
}

impl Drop for RunDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            tracing::warn!(
                "could not remove run directory {}: {error}; the next belt will remove it",
                self.path.display()
            );
        }
    }
}

/// Kills every process whose environment marks it as started by the run in `dir`, and logs each.
fn kill_marked(dir: &Path) {
    let Ok(mut system) = System::new() else {
        tracing::warn!(
            "cannot inspect processes; anything Factorio started for {} may still run",
            dir.display()
        );
        return;
    };
    let refresh = ProcessRefreshKind::nothing().with_environ(UpdateKind::Always);
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
    let marker = format!("{RUN_MARKER}={}", dir.display());
    for (pid, process) in system.processes() {
        if !process
            .environ()
            .iter()
            .any(|variable| variable.to_string_lossy() == marker)
        {
            continue;
        }
        let name = process.name().to_string_lossy();
        if process.kill() {
            tracing::warn!(
                "killed {name} (pid {pid}), left running by Factorio of {}",
                dir.display()
            );
        } else {
            tracing::warn!(
                "could not kill {name} (pid {pid}), left running by Factorio of {}",
                dir.display()
            );
        }
    }
}

struct RecordedProcess {
    pid: u32,
    start_time: u64,
}

impl RecordedProcess {
    fn read(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dir.join(PROCESS_FILE)).ok()?;
        let (pid, start_time) = text.trim().split_once(' ')?;
        Some(Self {
            pid: pid.parse().ok()?,
            start_time: start_time.parse().ok()?,
        })
    }

    /// Kills the process only if the same PID still has the recorded start time, so a reused
    /// PID is left alone.
    fn kill_if_alive(&self, dir: &Path) {
        let Ok(mut system) = System::new() else {
            tracing::warn!(
                "cannot inspect processes; Factorio {} from {} may still run",
                self.pid,
                dir.display()
            );
            return;
        };
        let pid = Pid::from_u32(self.pid);
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        match system.process(pid) {
            Some(process) if process.start_time() == self.start_time => {
                if process.kill() {
                    tracing::warn!(
                        "killed Factorio (pid {}) left running by an earlier belt in {}",
                        self.pid,
                        dir.display()
                    );
                } else {
                    tracing::warn!(
                        "could not kill Factorio (pid {}) left running by an earlier belt",
                        self.pid
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_belt_refuses_to_start_while_one_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let _first = Workspace::open(dir.path()).unwrap();

        let second = Workspace::open(dir.path());

        assert!(matches!(second, Err(WorkspaceError::Locked(_))));
    }

    #[test]
    fn the_sweep_kills_a_leftover_factorio_and_removes_its_run_directory() {
        let dir = tempfile::tempdir().unwrap();
        let mut leftover = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        {
            let workspace = Workspace::open(dir.path()).unwrap();
            let run = workspace.new_run().unwrap();
            run.record_process(leftover.id()).unwrap();
            std::mem::forget(run); // a crashed belt never removes its run directory
        }

        Workspace::open(dir.path()).unwrap();

        let killed = leftover.wait().unwrap().code().is_none();
        let runs_left = std::fs::read_dir(dir.path().join("runs")).unwrap().count();
        assert_eq!((killed, runs_left), (true, 0));
    }

    #[test]
    fn the_sweep_leaves_a_process_alone_whose_start_time_differs() {
        let dir = tempfile::tempdir().unwrap();
        let mut bystander = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        std::fs::create_dir_all(dir.path().join("runs/old")).unwrap();
        std::fs::write(
            dir.path().join("runs/old").join(PROCESS_FILE),
            format!("{} 1\n", bystander.id()),
        )
        .unwrap();

        Workspace::open(dir.path()).unwrap();

        let still_running = bystander.try_wait().unwrap().is_none();
        bystander.kill().unwrap();
        assert!(still_running);
    }
}
