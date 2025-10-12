//! The wrapper for the Factorio binary.

use std::{
    fs::{create_dir_all, read_to_string, write}, path::{Path, PathBuf}, process::Stdio, sync::{
        atomic::{AtomicBool, Ordering}, Arc
    }, time::Duration
};
use tokio::process::Command;

use crate::{
    benchmark::runner::FactorioOutput,
    core::{
        Result,
        error::{BenchmarkError, BenchmarkErrorKind},
        is_executable,
    },
};

use super::platform;

pub struct FactorioExecutor {
    executable_path: PathBuf,
}

pub struct FactorioRunSpec<'a> {
    pub save_file: &'a Path,
    pub ticks: u32,
    pub mods_dir: Option<&'a Path>,
    pub verbose_all_metrics: bool,
    pub headless: Option<bool>,
}

pub struct FactorioBlueprintRunSpec<'a> {
    pub save_file: &'a Path,
    pub blueprint_file: &'a Path,
    pub blueprint_stable_ticks: u32,
    pub mods_dir: &'a Path,
    pub data_dir: &'a Path,
    pub headless: Option<bool>,
}

impl FactorioExecutor {
    pub fn new(executable_path: PathBuf) -> Self {
        Self { executable_path }
    }

    /// Find the binary and create a FactorioExecutor with that path
    pub fn discover(explicit_path: Option<PathBuf>) -> Result<Self> {
        let path = Self::find_executable(explicit_path)?;
        Ok(Self::new(path))
    }

    /// Find the binary
    pub fn find_executable(explicit_path: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(path) = explicit_path {
            if path.exists() && path.is_file() {
                tracing::info!("Using explicit Factorio path: {}", path.display());
                return Ok(path);
            } else {
                let hint = if !is_executable(&path) {
                    Some("Make sure this is the path to the executable itself.")
                } else {
                    None
                };

                return Err(
                    BenchmarkError::from(BenchmarkErrorKind::FactorioNotFoundAtPath { path })
                        .with_hint(hint),
                );
            }
        }

        // Get possible locations of the binary, based on the user's operating system
        let candidates = platform::get_default_factorio_paths();

        // Check each candidate for if it exists
        for candidate in candidates {
            if candidate.exists() {
                tracing::info!("Found Factorio at: {}", candidate.display());
                return Ok(candidate);
            }
        }

        Err(BenchmarkErrorKind::FactorioNotFound.into())
    }

    /// Getter for the executable_path
    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    /// Public API for creating a command
    pub fn create_command(&self) -> Command {
        Command::new(&self.executable_path)
    }

    /// Sync Factorio's mods to the given save
    pub async fn sync_mods_for_save(&self, save_file: &Path) -> Result<()> {
        let mut cmd = self.create_command();

        cmd.args([
            "--sync-mods",
            save_file
                .to_str()
                .ok_or_else(|| BenchmarkErrorKind::InvalidSaveFileName {
                    path: save_file.to_path_buf(),
                })?,
        ]);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        tracing::debug!("Syncing mods to: {}", save_file.display());

        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

            let hint = if stdout_str.contains("already running")
                || stderr_str.contains("already running")
            {
                Some(
                    "Factorio might already be running. Please close any open Factorio instances."
                        .to_string(),
                )
            } else {
                None
            };

            return Err(
                BenchmarkError::from(BenchmarkErrorKind::FactorioProcessFailed {
                    code: output.status.code().unwrap_or(-1),
                })
                .with_hint(hint),
            );
        }

        tracing::debug!("Mod sync completed successfully");
        Ok(())
    }

    pub async fn run_for_blueprint(
        &self,
        spec: FactorioBlueprintRunSpec<'_>,
        running: &Arc<AtomicBool>,
    ) -> Result<PathBuf> {
        // first, lets write the mod files
        let bench_mod_dir = spec.mods_dir.join("benchmark-builder");
        create_dir_all(bench_mod_dir)?;
        let info_path = spec.mods_dir.join("benchmark-builder/info.json");
        if !info_path.exists() {
            write(info_path, r#"{
  "name": "benchmark-builder",
  "version": "0.1.0",
  "title": "Benchmark Builder",
  "author": "TwostepSA",
  "factorio_version": "2.0",
  "dependencies": ["base >= 2.0"],
  "description": "Automatically builds blueprints for benchmarking"
}"#)?;
        }
        let control_path = spec.mods_dir.join("benchmark-builder/control.lua");
        if !control_path.exists() {
            write(control_path, include_str!("control.lua"))?;
        }
        let bp_path = spec.mods_dir.join("benchmark-builder/bp.lua");
        
        write(bp_path, format!("local values = {{\n  bp_string = \"{}\",\n  save_after_ticks = {},\n  save_game_name = \"blueprint_benchmark\", bots = 0\n}}\nreturn values", read_to_string(spec.blueprint_file.to_str().unwrap())?, spec.blueprint_stable_ticks))?;

        // now delete the save we will create, if it exists
        let new_save_path = spec.data_dir.join(format!("saves/blueprint_{}.zip", spec.blueprint_file.file_stem().unwrap().to_str().unwrap()));
        if new_save_path.exists() {
            std::fs::remove_file(&new_save_path)?;
        }

        let mut cmd = self.create_command();

        cmd.args([
            "--mod-directory",
            spec.mods_dir
                    .to_str()
                    .ok_or_else(|| BenchmarkErrorKind::InvalidModsFileName {
                        path: spec.mods_dir.to_path_buf(),
                    })?,
            "--start-server",
            spec.save_file
                .to_str()
                .ok_or_else(|| BenchmarkErrorKind::InvalidSaveFileName {
                    path: spec.save_file.to_path_buf(),
                })?,
        ]);

        if let Some(headless) = spec.headless
            && headless
        {
            cmd.arg("--disable-audio");
        }

        // now run until the save file shows up
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        let mut did_kill = false;
        let poll_duration = Duration::from_secs(1);
        while child.try_wait().is_err() {
            if new_save_path.exists() {
                did_kill = true;
                let _ = child.start_kill();
                break;
            }
            if !running.load(Ordering::SeqCst) {
                tracing::info!("Ctrl+C received. Killing Factorio");
                let _ = child.start_kill();
                break;
            }
            tokio::time::sleep(poll_duration).await;
        }
        let output = child.wait_with_output().await?;
        if !output.status.success() && !did_kill {
            let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

            let hint = if stdout_str.contains("already running")
                || stderr_str.contains("already running")
            {
                Some(
                    "Factorio might already be running. Please close any open Factorio instances."
                        .to_string(),
                )
            } else {
                None
            };

            return Err(
                BenchmarkError::from(BenchmarkErrorKind::FactorioProcessFailed {
                    code: output.status.code().unwrap_or(-1),
                })
                .with_hint(hint),
            );
        }

        Ok(new_save_path)
    }

    pub async fn run_for_ticks(
        &self,
        spec: FactorioRunSpec<'_>,
        running: &Arc<AtomicBool>,
    ) -> Result<FactorioOutput> {
        let mut cmd = self.create_command();

        cmd.args([
            "--benchmark",
            spec.save_file
                .to_str()
                .ok_or_else(|| BenchmarkErrorKind::InvalidSaveFileName {
                    path: spec.save_file.to_path_buf(),
                })?,
            "--benchmark-ticks",
            &spec.ticks.to_string(),
            "--benchmark-runs",
            "1", // Always run single benchmark
        ]);

        if let Some(headless) = spec.headless
            && headless
        {
            tracing::debug!("Running headless mode, not disabling audio");
        } else {
            cmd.arg("--disable-audio");
        }

        if spec.verbose_all_metrics {
            cmd.arg("--benchmark-verbose");
            cmd.arg("all");
        }

        // Run with the argument --mod-directory if a mod-directory was given
        if let Some(mods_dir) = spec.mods_dir {
            cmd.arg("--mod-directory");
            cmd.arg(
                mods_dir
                    .to_str()
                    .ok_or_else(|| BenchmarkErrorKind::InvalidModsFileName {
                        path: mods_dir.to_path_buf(),
                    })?,
            );
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let poll_duration = Duration::from_secs(1);
        while child.try_wait().is_err() {
            if !running.load(Ordering::SeqCst) {
                tracing::info!("Ctrl+C received. Killing Factorio");
                let _ = child.start_kill();
                break;
            }
            tokio::time::sleep(poll_duration).await;
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

            let hint = if stdout_str.contains("already running")
                || stderr_str.contains("already running")
            {
                Some(
                    "Factorio might already be running. Please close any open Factorio instances."
                        .to_string(),
                )
            } else {
                None
            };

            return Err(
                BenchmarkError::from(BenchmarkErrorKind::FactorioProcessFailed {
                    code: output.status.code().unwrap_or(-1),
                })
                .with_hint(hint),
            );
        }

        let stdout = String::from_utf8(output.stdout)?;
        const VERBOSE_HEADER: &str = "tick,timestamp,wholeUpdate";

        if let Some(index) = stdout.find(VERBOSE_HEADER) {
            let (summary, verbose_part) = stdout.split_at(index);

            let cleaned_verbose_data: String = verbose_part
                .lines()
                .filter(|line| line.starts_with("tick,") || line.starts_with('t'))
                .collect::<Vec<&str>>()
                .join("\n");

            Ok(FactorioOutput {
                summary: summary.to_string(),
                verbose_data: Some(cleaned_verbose_data),
            })
        } else {
            Ok(FactorioOutput {
                summary: stdout,
                verbose_data: None,
            })
        }
    }
}
