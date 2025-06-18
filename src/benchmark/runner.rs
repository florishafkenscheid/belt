use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use indicatif::{ProgressBar, ProgressStyle};

use crate::benchmark::parser;
use crate::benchmark::parser::BenchmarkResult;
use crate::core::FactorioExecutor;
use crate::core::{BenchmarkError, Result};
use super::{BenchmarkConfig};

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    factorio: FactorioExecutor,
}

impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    pub async fn run_all(&self, save_files: Vec<PathBuf>) -> Result<Vec<BenchmarkResult>> {
        let progress = ProgressBar::new(save_files.len() as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}"
            )
            .map_err(|e| BenchmarkError::ProgressBarError(e.to_string()))?
            .progress_chars("==")
        );
        progress.enable_steady_tick(Duration::from_millis(100));

        let mut all_results = Vec::new(); 

        for (i, save_file) in save_files.iter().enumerate() {
            let save_name = save_file.file_stem()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName { path: save_file.clone() })?
                .to_string_lossy()
                .to_string();

            progress.set_position(i as u64);
            progress.set_message(format!("{}", save_name));

            match self.run_benchmark_for_save(&save_file).await {
                Ok(result) => {
                    all_results.push(result);
                }
                Err(err) => {
                    tracing::error!("Benchmark failed for {}: {}", save_name, err);
                    continue;
                }
            }
        }

        progress.finish_with_message("Benchmarking complete!");
        Ok(all_results)
    }

    async fn run_benchmark_for_save(&self, save_file: &Path) -> Result<BenchmarkResult> {
        // If mods_file is not set, sync mods with the given save file
        if self.config.mods_dir == None {
            self.sync_mods_for_save(save_file).await?;
        };

        let log_output = self.execute_factorio_benchmark(save_file).await?;
        let result = parser::parse_benchmark_log(&log_output, save_file, &self.config).map_err(|_| BenchmarkError::ParseError { reason: "".to_string() })?;
        Ok(result)
    }

    async fn sync_mods_for_save(&self, save_file: &Path) -> Result<()> {
        let mut cmd = self.factorio.create_command();

        cmd.args(&[
            "--sync-mods",
            save_file.to_str()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName { 
                    path: save_file.to_path_buf()
                })?
        ]);

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        tracing::debug!("Syncing mods to: {}", save_file.display());

        let child = cmd.spawn()?;

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string()
            });
        }

        tracing::debug!("Mod sync completed successfully");
        Ok(())
    }

    async fn execute_factorio_benchmark(&self, save_file: &Path) -> Result<String> {
        let mut cmd = self.factorio.create_command(); 

        cmd.args(&[
            "--benchmark", save_file.to_str().ok_or_else(|| BenchmarkError::InvalidSaveFileName { path: save_file.to_path_buf() })?,
            "--benchmark-ticks", &self.config.ticks.to_string(),
            "--benchmark-runs", &self.config.runs.to_string(),
            "--disable-audio",
        ]);

        if let Some(mods_file) = &self.config.mods_dir {
            cmd.arg("--mod-directory");
            cmd.arg(
                mods_file.to_str().ok_or_else(|| BenchmarkError::InvalidModsFileName { path: mods_file.clone() })?
            );
            tracing::debug!("Set the mod list to: {}", mods_file.display());
        }

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        tracing::debug!("Executing: {:?}", cmd);

        let child = cmd.spawn()?;

        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                stderr: stderr.to_string(),
            });
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| BenchmarkError::InvalidUtf8Output)?;

        Ok(stdout)
    }
}
