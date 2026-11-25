//! Terminal output, as in 4.3.1: levelled, coloured `tracing` lines on stdout, and an
//! `indicatif` progress bar for benchmark and sanitize runs. Log lines are printed above a
//! running bar instead of through it.

use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;

/// The running progress bar, which a log line hides while it prints. A finished bar is no
/// longer here: it stays on screen as it was, and later lines print below it.
static ACTIVE: Mutex<Option<ProgressBar>> = Mutex::new(None);

/// The log level: `INFO`, or with `--verbose` `DEBUG` for belt's own lines (which include
/// Factorio's output). Other crates stay at `INFO`, so their internals don't drown it.
fn level(verbose: bool) -> Targets {
    let belt = if verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    Targets::new()
        .with_default(LevelFilter::INFO)
        .with_target(env!("CARGO_CRATE_NAME"), belt)
}

/// The installed logger. Its level starts from the `--verbose` flag, so errors in reading the
/// configuration are logged too, and is settled once the configuration is read.
pub struct Logger {
    level: reload::Handle<Targets, tracing_subscriber::Registry>,
}

impl Logger {
    /// Installs the logger. Call once, first thing.
    pub fn install(verbose: bool) -> Self {
        let (filter, level) = reload::Layer::new(self::level(verbose));
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_writer(LogWriter))
            .init();
        Self { level }
    }

    /// Sets the level from the resolved `verbose` setting (flag, environment or config file).
    pub fn set_verbose(&self, verbose: bool) {
        if let Err(error) = self.level.modify(|filter| *filter = level(verbose)) {
            tracing::warn!("cannot change the log level: {error}");
        }
    }
}

/// Writes each log line to stdout while the progress bars are hidden.
struct LogWriter;

/// One log line, written out when the formatter drops it.
struct LogLine(Vec<u8>);

impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = LogLine;

    fn make_writer(&'a self) -> Self::Writer {
        LogLine(Vec::new())
    }
}

impl Write for LogLine {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogLine {
    fn drop(&mut self) {
        // A log line that can't be written has nowhere else to go.
        let write = || {
            let _ = std::io::stdout().lock().write_all(&self.0);
        };
        match ACTIVE.lock().expect("lock").as_ref() {
            Some(bar) => bar.suspend(write),
            None => write(),
        }
    }
}

/// A progress bar over `total` jobs, showing the current job and the estimated time left. One
/// runs at a time.
pub struct Progress {
    bar: ProgressBar,
    started: Instant,
    total: usize,
}

impl Progress {
    pub fn new(total: usize) -> Self {
        let style = ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )
        .expect("a valid template")
        .progress_chars("==");
        let bar = ProgressBar::new(total as u64).with_style(style);
        bar.enable_steady_tick(Duration::from_millis(100));
        *ACTIVE.lock().expect("lock") = Some(bar.clone());
        Self {
            bar,
            started: Instant::now(),
            total,
        }
    }

    /// Shows that job `done` (0-based: the number of jobs before it) is running, as `label`.
    /// From the second job on, the label carries the time left at the average pace so far.
    /// Each job is also logged at debug level, for `--verbose` and output that isn't a terminal.
    pub fn start(&self, done: usize, label: &str) {
        tracing::debug!("[{}/{}] {label}", done + 1, self.total);
        self.bar.set_position(done as u64);
        if done == 0 {
            self.bar.set_message(label.to_string());
            return;
        }
        let per_job = self.started.elapsed() / done as u32;
        let left = per_job * (self.total - done) as u32;
        self.bar
            .set_message(format!("{label} [ETA: {}]", format_duration(left)));
    }

    /// Fills the bar and leaves it on screen with `message`.
    pub fn finish(self, message: &'static str) {
        self.bar.finish_with_message(message);
    }

    /// Leaves the bar on screen where it stopped, with `message`.
    pub fn stop(self, message: &'static str) {
        self.bar.abandon_with_message(message);
    }
}

/// However the command ends, the bar stays on screen where it got to (an error leaves it
/// unfinished), and later log lines no longer wait for it.
impl Drop for Progress {
    fn drop(&mut self) {
        if !self.bar.is_finished() {
            self.bar.abandon();
        }
        *ACTIVE.lock().expect("lock") = None;
    }
}

/// `42s`, `3m5s` or `2h7m`, as 4.3.1 showed the time left.
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m{}s", seconds / 60, seconds % 60),
        _ => format!("{}h{}m", seconds / 3600, seconds % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_is_seconds_then_minutes_then_hours() {
        let shown = [42, 185, 7620].map(|seconds| format_duration(Duration::from_secs(seconds)));

        assert_eq!(shown, ["42s", "3m5s", "2h7m"]);
    }
}
