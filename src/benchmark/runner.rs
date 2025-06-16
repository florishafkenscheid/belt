use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use indicatif::{ProgressBar, ProgressStyle};
use anyhow::{Context, Result};

use crate::benchmark::parser;
use crate::benchmark::parser::BenchmarkResult;
use crate::core::FactorioExecutor;
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
            )?
            .progress_chars("==")
        );
        progress.enable_steady_tick(Duration::from_millis(100));

        let mut all_results = Vec::new();

        for (i, save_file) in save_files.iter().enumerate() {
            let save_name = save_file.file_stem()
                .context("Invalid save file name")?
                .to_string_lossy()
                .to_string();

            progress.set_position(i as u64);
            progress.set_message(format!("{}", save_name));

            match self.run_benchmark_for_save(save_file).await {
                Ok(mut results) => {
                    all_results.append(&mut results);
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

    async fn run_benchmark_for_save(&self, save_file: &Path) -> Result<Vec<BenchmarkResult>> {
        self.sync_mods_for_save(save_file).await?;
        let log_output = self.execute_factorio_benchmark(save_file).await?;
        let results = parser::parse_benchmark_log(&log_output, save_file, &self.config)?;
        Ok(results)
    }

    async fn sync_mods_for_save(&self, save_file: &Path) -> Result<()> {
        let mut cmd = self.factorio.create_command();

        cmd.args(&[
            "--sync-mods", save_file.to_str().unwrap()
        ]);

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        tracing::debug!("Syncing mods: {:?}", cmd);

        let child = cmd.spawn()
            .context("Failed to start Factorio mod sync process")?;

        let output = child.wait_with_output().await
            .context("Failed to wait for Factorio mod sync process")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Factorio mod sync failed: {}", stderr);
        }

        tracing::debug!("Mod sync completed successfully");
        Ok(())
    }

    async fn execute_factorio_benchmark(&self, save_file: &Path) -> Result<String> {
        let mut cmd = self.factorio.create_command(); 

        cmd.args(&[
            "--benchmark", save_file.to_str().unwrap(),
            "--benchmark-ticks", &self.config.ticks.to_string(),
            "--benchmark-runs", &self.config.runs.to_string(),
            "--disable-audio",
        ]);

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        tracing::debug!("Executing: {:?}", cmd);

        let child = cmd.spawn()
            .context("Failed to start Factorio process")?;

        let output = child.wait_with_output().await
            .context("Failed to wait for Factorio process")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Factorio process failed: {}", stderr);
        }

        let stdout = String::from_utf8(output.stdout)
            .context("Factorio output is not valid UTF-8")?;

        Ok(stdout)
    }
}
