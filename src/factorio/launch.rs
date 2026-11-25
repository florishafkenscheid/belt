//! Which Factorio launch options each command relies on, and from which Factorio version they
//! exist. Factorio's version is read before anything else, so an old Factorio is refused up front
//! instead of answering a launch with its usage text.
//!
//! Known: every option here exists in 2.1.19 and 2.1.25, except the test mode (`--test-save`,
//! `--test-ticks`), which 2.1.19 lacks and which arrived in 2.1.24. Versions before 2.1.19 weren't
//! checked; B.E.L.T. targets 2.1, so 2.1.0 is the floor.

use crate::version::Version;

/// The oldest Factorio B.E.L.T. targets.
const FLOOR: Version = Version::new(2, 1, 0);

/// A B.E.L.T. command, for the options it launches Factorio with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Benchmark,
    Blueprint,
    Sanitize,
}

impl Command {
    fn name(self) -> &'static str {
        match self {
            Self::Benchmark => "benchmark",
            Self::Blueprint => "blueprint",
            Self::Sanitize => "sanitize",
        }
    }

    /// Every launch option the command needs, mod syncing included.
    fn options(self) -> &'static [LaunchOption] {
        match self {
            Self::Benchmark => &[LaunchOption::SyncMods, LaunchOption::Benchmark],
            Self::Blueprint => &[LaunchOption::SyncMods, LaunchOption::StartServer],
            Self::Sanitize => &[LaunchOption::SyncMods, LaunchOption::TestSave],
        }
    }
}

/// A launch option B.E.L.T. relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchOption {
    /// `--benchmark`, `--benchmark-ticks`, `--benchmark-runs`, `--benchmark-verbose`.
    Benchmark,
    /// `--sync-mods`.
    SyncMods,
    /// `--start-server`, `--server-settings`, `--bind`, `--port`.
    StartServer,
    /// `--test-save`, `--test-ticks`.
    TestSave,
}

impl LaunchOption {
    fn flag(self) -> &'static str {
        match self {
            Self::Benchmark => "--benchmark",
            Self::SyncMods => "--sync-mods",
            Self::StartServer => "--start-server",
            Self::TestSave => "--test-save",
        }
    }

    fn minimum(self) -> Version {
        match self {
            Self::TestSave => Version::new(2, 1, 24),
            Self::Benchmark | Self::SyncMods | Self::StartServer => FLOOR,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("{command} needs Factorio {minimum} or newer (for {flag}); this Factorio is {found}")]
pub struct TooOld {
    command: &'static str,
    minimum: Version,
    flag: &'static str,
    found: Version,
}

/// Refuses a Factorio older than the newest minimum among the command's options.
pub fn check(found: Version, command: Command) -> Result<(), TooOld> {
    let Some(option) = command
        .options()
        .iter()
        .copied()
        .max_by_key(|option| option.minimum())
    else {
        return Ok(());
    };
    if found < option.minimum() {
        return Err(TooOld {
            command: command.name(),
            minimum: option.minimum(),
            flag: option.flag(),
            found,
        });
    }
    Ok(())
}

/// The version in `factorio --version` output (`Version: 2.1.25 (build 88022, linux64, …)`).
pub fn parse_version(output: &str) -> Option<Version> {
    let line = output
        .lines()
        .find_map(|line| line.strip_prefix("Version: "))?;
    Version::parse(line.split_whitespace().next()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_is_refused_before_factorio_2_1_24_naming_the_version_and_option() {
        let error = check(Version::new(2, 1, 19), Command::Sanitize).unwrap_err();

        assert_eq!(
            error.to_string(),
            "sanitize needs Factorio 2.1.24 or newer (for --test-save); this Factorio is 2.1.19"
        );
    }

    #[test]
    fn benchmark_and_blueprint_run_on_2_1_19() {
        let checks = [
            check(Version::new(2, 1, 19), Command::Benchmark),
            check(Version::new(2, 1, 19), Command::Blueprint),
        ];

        assert_eq!(checks, [Ok(()), Ok(())]);
    }

    #[test]
    fn factorio_2_0_is_refused_for_every_command() {
        let found = Version::new(2, 0, 76);
        let refused = [Command::Benchmark, Command::Blueprint, Command::Sanitize]
            .map(|command| check(found, command).is_err());

        assert_eq!(refused, [true, true, true]);
    }

    #[test]
    fn the_version_is_read_from_factorio_version_output() {
        let output = "Version: 2.1.19 (build 87445, linux64, full)\nVersion: 64\nMap input version: 1.0.0-0\n";

        assert_eq!(parse_version(output), Some(Version::new(2, 1, 19)));
    }
}
