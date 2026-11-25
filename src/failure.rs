//! Per-save failures: collected while a command keeps going, listed at the end.

use std::fmt;

use crate::protocol::Unplaced;

/// Why one save (or one blueprint) failed, with the end of Factorio's output.
#[derive(Debug, Clone, PartialEq)]
pub struct RunFailure {
    pub reason: FailureReason,
    pub log_tail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FailureReason {
    /// Factorio couldn't be started, or a file around the run couldn't be prepared.
    Setup(String),
    /// Factorio exited with this code (`None`: killed by a signal).
    Exited(Option<i32>),
    /// Factorio exited 0 but the expected result never arrived.
    NoResult,
    /// The mod reported an error.
    Mod(String),
    /// The mod spoke an unexpected dialect.
    Protocol(String),
    /// Factorio's output didn't hold what the benchmark needs.
    Output(String),
    /// Ctrl-C stopped the run.
    Interrupted,
    /// The save's mods can't be provided.
    Mods(String),
    /// Blueprint entities that couldn't be placed.
    Unplaced(Vec<Unplaced>),
}

impl RunFailure {
    pub fn new(reason: FailureReason) -> Self {
        Self {
            reason,
            log_tail: Vec::new(),
        }
    }

    pub fn with_tail(reason: FailureReason, log_tail: Vec<String>) -> Self {
        Self { reason, log_tail }
    }
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(message) => write!(f, "{message}"),
            Self::Exited(Some(code)) => write!(f, "Factorio exited with code {code}"),
            Self::Exited(None) => write!(f, "Factorio was killed by a signal"),
            Self::NoResult => write!(f, "Factorio exited without delivering a result"),
            Self::Mod(message) => write!(f, "the mod reported: {message}"),
            Self::Protocol(message) => write!(f, "{message}"),
            Self::Output(message) => write!(f, "unexpected Factorio output: {message}"),
            Self::Interrupted => write!(f, "interrupted"),
            Self::Mods(message) => write!(f, "{message}"),
            Self::Unplaced(unplaced) => {
                write!(f, "{} entities could not be placed:", unplaced.len())?;
                for entity in unplaced {
                    write!(
                        f,
                        "\n    {} at ({}, {}): {}",
                        entity.name, entity.x, entity.y, entity.reason
                    )?;
                }
                Ok(())
            }
        }
    }
}

/// Failures of one command, in the order they happened.
#[derive(Debug, Default)]
pub struct Failures {
    entries: Vec<(String, RunFailure)>,
}

impl Failures {
    pub fn add(&mut self, name: &str, failure: RunFailure) {
        tracing::error!("{name} failed: {}", failure.reason);
        self.entries.push((name.to_string(), failure));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Each failed save with its reason and log tail, for the end of a command. One log
    /// message, so it has no final newline.
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("{} failed:", self.entries.len())];
        for (name, failure) in &self.entries {
            lines.push(format!("\n{name}: {}", failure.reason));
            lines.extend(failure.log_tail.iter().map(|line| format!("  | {line}")));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_saves_are_listed_with_their_log_tail() {
        let mut failures = Failures::default();
        failures.add(
            "a",
            RunFailure::with_tail(FailureReason::Exited(Some(1)), vec!["Error: boom".into()]),
        );

        assert_eq!(
            failures.summary(),
            "1 failed:\n\na: Factorio exited with code 1\n  | Error: boom"
        );
    }
}
