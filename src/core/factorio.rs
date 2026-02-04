//! The wrapper for the Factorio binary.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::process::Command;

use crate::{
    benchmark::runner::FactorioOutput,
    core::{
        Result,
        error::{BenchmarkError, BenchmarkErrorKind},
        is_executable, utils,
    },
};

use super::platform;

pub struct FactorioExecutor {
    executable_path: PathBuf,
}

pub struct FactorioTickRunSpec<'a> {
    pub save_file: &'a Path,
    pub ticks: u32,
    pub mods_dir: Option<&'a Path>,
    pub verbose_all_metrics: bool,
    pub headless: Option<bool>,
}

pub struct FactorioSaveRunSpec<'a> {
    pub base_save_file: &'a Path,
    pub new_save_name: String,
    pub mods_dir: Option<&'a Path>,
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
                tracing::debug!("Found Factorio at: {}", candidate.display());
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

    pub async fn run_for_ticks(&self, spec: FactorioTickRunSpec<'_>) -> Result<FactorioOutput> {
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

            tracing::debug!("Out: {stdout_str}");
            tracing::debug!("Err: {stderr_str}");

            return Err(
                BenchmarkError::from(BenchmarkErrorKind::FactorioProcessFailed {
                    code: output.status.code().unwrap_or(-1),
                })
                .with_hint(hint),
            );
        }

        let summary = String::from_utf8_lossy(&output.stderr).to_string()
            + String::from_utf8_lossy(&output.stdout).as_ref();

        const VERBOSE_HEADER: &str = "tick,timestamp,wholeUpdate";

        if let Some(index) = summary.find(VERBOSE_HEADER) {
            let (summary, verbose_part) = summary.split_at(index);

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
                summary,
                verbose_data: None,
            })
        }
    }

    pub async fn run_for_save(
        &self,
        spec: FactorioSaveRunSpec<'_>,
        running: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut cmd = self.create_command();

        cmd.args([
            "--load-game",
            spec.base_save_file.to_str().ok_or_else(|| {
                BenchmarkErrorKind::InvalidSaveFileName {
                    path: spec.base_save_file.to_path_buf(),
                }
            })?,
            "--disable-migration-window",
        ]);

        if let Some(headless) = spec.headless
            && headless
        {
            tracing::debug!("Running headless mode, not disabling audio");
        } else {
            cmd.arg("--disable-audio");
        }

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

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::debug!("Exited with: {status}");
                    break;
                }
                Ok(None) => {
                    if utils::check_save_file(format!("_autosave-{}", spec.new_save_name.clone()))
                        .is_some()
                    {
                        child.start_kill()?;
                        break;
                    }

                    if !running.load(Ordering::SeqCst) {
                        tracing::info!("Ctrl+C received. Killing Factorio");
                        child.start_kill()?;
                        break;
                    }
                    tokio::time::sleep(poll_duration).await;
                }
                Err(err) => {
                    tracing::error!("Error while polling child: {err}");
                }
            }
        }

        let output = child.wait_with_output().await?;

        if !output.status.success() && output.status.code().is_some() {
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

        Ok(())
    }
}
