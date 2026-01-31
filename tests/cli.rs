use std::{error::Error, fs::File};

use assert_cmd::cargo::cargo_bin_cmd;
use tempfile::tempdir;

#[test]
fn test_benchmark_command_creates_output_files() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    let save_file_path = temp_path.join("test_save.zip");
    File::create(&save_file_path)?;

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
