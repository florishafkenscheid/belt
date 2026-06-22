use std::{
    error::Error,
    fs::File,
    path::{Path, PathBuf},
};

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

fn create_fake_factorio(temp_path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let fake_factorio_exe = temp_path.join("factorio");
    let fake_output =
        "Performed 10 updates in 100.000 ms\navg: 10.000 ms, min: 10.000 ms, max: 10.000 ms";
    std::fs::write(
        &fake_factorio_exe,
        format!("#!/bin/sh\necho '{fake_output}'"),
    )?;

    #[cfg(unix)]
    {
        use std::{fs::Permissions, os::unix::fs::PermissionsExt};

        let perms = Permissions::from_mode(0o755);
        std::fs::set_permissions(&fake_factorio_exe, perms)?;
    }

    Ok(fake_factorio_exe)
}

#[test]
fn test_blueprint_help_includes_mining_module_replacement_options() -> Result<(), Box<dyn Error>> {
    let mut cmd = cargo_bin_cmd!("belt");

    let output = cmd.arg("blueprint").arg("--help").output()?;
    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--mining-module-replacement"));
    assert!(stdout.contains("--mining-module-replacement-quality"));
    assert!(stdout.contains("[default: speed-module-3]"));
    assert!(stdout.contains("[default: legendary]"));

    Ok(())
}

#[test]
fn test_benchmark_help_lists_saves_dir_as_argument() -> Result<(), Box<dyn Error>> {
    let mut cmd = cargo_bin_cmd!("belt");

    let output = cmd.arg("benchmark").arg("--help").output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[SAVES_DIR]"));
    assert!(stdout.contains("Benchmark Options:"));
    assert!(stdout.contains("Global Options:"));
    assert!(stdout.contains("--record-cpu"));
    assert!(!stdout.contains("--record-cpu <"));
    assert!(stdout.contains("--headless"));
    assert!(!stdout.contains("--init-config"));

    Ok(())
}

#[test]
fn test_benchmark_command_accepts_saves_dir_from_config() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    let save_file_path = temp_path.join("test_save.zip");
    File::create(&save_file_path)?;

    let fake_factorio_exe = create_fake_factorio(temp_path)?;

    let config_path = temp_path.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[benchmark]
saves_dir = "{}"
"#,
            save_file_path.display()
        ),
    )?;

    let mut cmd = cargo_bin_cmd!("belt");

    cmd.arg("benchmark")
        .arg("--config")
        .arg(config_path)
        .arg("--output")
        .arg(temp_path)
        .arg("--factorio-path")
        .arg(&fake_factorio_exe)
        .arg("--runs")
        .arg("1")
        .arg("--ticks")
        .arg("10");

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

#[test]
fn test_benchmark_command_creates_output_files() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    let save_file_path = temp_path.join("test_save.zip");
    File::create(&save_file_path)?;

    let fake_factorio_exe = create_fake_factorio(temp_path)?;

    let mut cmd = cargo_bin_cmd!("belt");

    cmd.arg("benchmark")
        .arg(&save_file_path)
        .arg("--output")
        .arg(temp_path)
        .arg("--factorio-path")
        .arg(&fake_factorio_exe)
        .arg("--runs")
        .arg("1")
        .arg("--ticks")
        .arg("10");

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let csv_path = temp_path.join("results.csv");
    let md_path = temp_path.join("results.md");

    assert!(
        csv_path.exists(),
        "results.csv should have been created in the temporary directory"
    );
    assert!(
        md_path.exists(),
        "results.md should have been created in the temporary directory"
    );

    Ok(())
}

#[test]
fn test_benchmark_command_accepts_record_cpu_toggle() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    let save_file_path = temp_path.join("test_save.zip");
    File::create(&save_file_path)?;

    let fake_factorio_exe = create_fake_factorio(temp_path)?;

    let mut cmd = cargo_bin_cmd!("belt");

    cmd.arg("benchmark")
        .arg(&save_file_path)
        .arg("--output")
        .arg(temp_path)
        .arg("--factorio-path")
        .arg(&fake_factorio_exe)
        .arg("--runs")
        .arg("1")
        .arg("--ticks")
        .arg("10")
        .arg("--record-cpu");

    let output = cmd.output()?;
    assert!(
        output.status.success(),
        "Command should succeed. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

#[test]
fn test_analyze_subcommand_is_removed() -> Result<(), Box<dyn Error>> {
    let mut cmd = cargo_bin_cmd!("belt");
    cmd.arg("analyze").arg("--help");

    let output = cmd.output()?;
    assert!(
        !output.status.success(),
        "Analyze subcommand should be unavailable"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand") || stderr.contains("unknown subcommand"),
        "Expected clap to reject removed subcommand. Stderr: {stderr}"
    );

    Ok(())
}
