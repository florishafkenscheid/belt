//! How `--verbose` sets the log level. No Factorio needed: belt logs the saves it found before
//! it looks for Factorio.

mod common;

use std::process::{Command, Output};

/// Runs `belt benchmark` on a directory holding `a.zip`, with no Factorio to find.
fn benchmark(global: &[&str], environment: &[(&str, &str)]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let saves = dir.path().join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("a.zip"), "").unwrap();
    std::fs::write(dir.path().join("config.toml"), "").unwrap();
    Command::new(env!("CARGO_BIN_EXE_belt"))
        .env("HOME", dir.path())
        .env("XDG_CONFIG_HOME", dir.path().join(".config"))
        .env("XDG_DATA_HOME", dir.path().join(".local/share"))
        .envs(environment.iter().copied())
        .arg("--config")
        .arg(dir.path().join("config.toml"))
        .arg("--factorio-path")
        .arg(dir.path().join("no-factorio"))
        .args(global)
        .arg("benchmark")
        .arg(&saves)
        .output()
        .unwrap()
}

fn debug_entries(output: &Output) -> Vec<String> {
    common::log_entries(&common::log(output))
        .into_iter()
        .filter(|entry| entry.starts_with("DEBUG "))
        .collect()
}

#[test]
fn the_log_shows_information_and_up_by_default() {
    let output = benchmark(&[], &[]);

    assert_eq!(debug_entries(&output), Vec::<String>::new());
}

#[test]
fn verbose_logs_debug_lines() {
    let output = benchmark(&["--verbose"], &[]);

    assert_eq!(debug_entries(&output), ["DEBUG   - a"]);
}

#[test]
fn verbose_from_the_environment_logs_debug_lines() {
    let output = benchmark(&[], &[("BELT_GLOBAL__VERBOSE", "true")]);

    assert_eq!(debug_entries(&output), ["DEBUG   - a"]);
}
