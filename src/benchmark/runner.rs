use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::Instant;

use super::{BenchmarkConfig, RunOrder};
use crate::benchmark::parser;
use crate::benchmark::parser::{BenchmarkResult, BenchmarkRun};
use crate::core::FactorioExecutor;
use crate::core::{BenchmarkError, Result};

#[derive(Debug, Clone)]
struct ExecutionJob {
    save_file: PathBuf,
    run_index: usize,
}

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    factorio: FactorioExecutor,
}

impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    pub async fn run_all(&self, save_files: Vec<PathBuf>) -> Result<Vec<BenchmarkResult>> {
        let execution_schedule = self.create_execution_schedule(&save_files);
        let total_jobs = execution_schedule.len();
        let start_time = Instant::now();

        let progress = ProgressBar::new(total_jobs as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )
            .map_err(|e| BenchmarkError::ProgressBarError(e.to_string()))?
            .progress_chars("=="),
        );
        progress.enable_steady_tick(Duration::from_millis(100));

        // Initialize results structure
        let mut results_map: HashMap<String, Vec<BenchmarkRun>> = HashMap::new();
        for save_file in &save_files {
            let save_name = save_file
                .file_stem()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: save_file.clone(),
                })?
                .to_string_lossy()
                .to_string();
            results_map.insert(save_name, Vec::new());
        }

        // Execute jobs according to schedule
        for (job_index, job) in execution_schedule.iter().enumerate() {
            let save_name = job
                .save_file
                .file_stem()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: job.save_file.clone(),
                })?
                .to_string_lossy()
                .to_string();

            progress.set_position(job_index as u64);

            let eta_message = if job_index > 0 {
                let elapsed = start_time.elapsed();
                let avg_time_per_job = elapsed / job_index as u32;
                let remaining_jobs = total_jobs - job_index;
                let estimated_remaining = avg_time_per_job * remaining_jobs as u32;

                format!(
                    "{} (run {}) [ETA: {}]",
                    save_name,
                    job.run_index + 1,
                    format_duration(estimated_remaining)
                )
            } else {
                format!("{} (run {})", save_name, job.run_index + 1)
            };

            progress.set_message(eta_message);

            match self.run_single_benchmark(&job.save_file).await {
                Ok(run) => {
                    if let Some(runs) = results_map.get_mut(&save_name) {
                        runs.push(run);
                    }
                }
                Err(err) => {
                    tracing::error!(
                        "Benchmark failed for {} run {}: {}",
                        save_name,
                        job.run_index + 1,
                        err
                    );
                    continue;
                }
            }
        }

        progress.finish_with_message("Benchmarking complete!");

        let mut all_results = Vec::new();
        for save_file in save_files {
            let save_name = save_file
                .file_stem()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: save_file.clone(),
                })?
                .to_string_lossy()
                .to_string();

            if let Some(runs) = results_map.remove(&save_name) {
                if !runs.is_empty() {
                    all_results.push(BenchmarkResult {
                        save_name: save_name.clone(),
                        ticks: self.config.ticks,
                        runs,
                        factorio_version: "unknown".to_string(),
                        platform: crate::core::platform::get_os_info(),
                    });
                }
            }
        }

        // Sort by performance
        all_results.sort_by(|a, b| {
            let avg_a: f64 =
                a.runs.iter().map(|run| run.effective_ups).sum::<f64>() / a.runs.len() as f64;
            let avg_b: f64 =
                b.runs.iter().map(|run| run.effective_ups).sum::<f64>() / b.runs.len() as f64;

            avg_a
                .partial_cmp(&avg_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(all_results)
    }

    fn create_execution_schedule(&self, save_files: &[PathBuf]) -> Vec<ExecutionJob> {
        let mut schedule = Vec::new();

        match self.config.run_order {
            RunOrder::Grouped => {
                // Current behavior: A,A,A,B,B,B
                for save_file in save_files {
                    for run_index in 0..self.config.runs {
                        schedule.push(ExecutionJob {
                            save_file: save_file.clone(),
                            run_index: run_index as usize,
                        });
                    }
                }
            }
            RunOrder::Sequential => {
                // Alternating: A,B,A,B,A,B
                for run_index in 0..self.config.runs {
                    for save_file in save_files {
                        schedule.push(ExecutionJob {
                            save_file: save_file.clone(),
                            run_index: run_index as usize,
                        });
                    }
                }
            }
            RunOrder::Random => {
                for save_file in save_files {
                    for run_index in 0..self.config.runs {
                        schedule.push(ExecutionJob {
                            save_file: save_file.clone(),
                            run_index: run_index as usize,
                        });
                    }
                }
                let mut rng = rand::rng();
                schedule.shuffle(&mut rng);
            }
        }

        tracing::debug!(
            "Created execution schedule with {} jobs using {:?} order",
            schedule.len(),
            self.config.run_order
        );

        schedule
    }

    async fn run_single_benchmark(&self, save_file: &Path) -> Result<BenchmarkRun> {
        // If mods_file is not set, sync mods with the given save file
        if self.config.mods_dir.is_none() {
            self.sync_mods_for_save(save_file).await?;
        }

        let log_output = self.execute_single_factorio_benchmark(save_file).await?;
        let result =
            parser::parse_benchmark_log(&log_output, save_file, &self.config).map_err(|_| {
                BenchmarkError::ParseError {
                    reason: "Failed to parse benchmark log".to_string(),
                }
            })?;

        // Extract the single run (since we're only running 1 benchmark at a time)
        if result.runs.len() != 1 {
            return Err(BenchmarkError::ParseError {
                reason: format!("Expected 1 run, got {}", result.runs.len()),
            });
        }

        Ok(result.runs.into_iter().next().unwrap())
    }

    async fn sync_mods_for_save(&self, save_file: &Path) -> Result<()> {
        let mut cmd = self.factorio.create_command();

        cmd.args([
            "--sync-mods",
            save_file
                .to_str()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: save_file.to_path_buf(),
                })?,
        ]);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        tracing::debug!("Syncing mods to: {}", save_file.display());

        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let err = output.stdout.last().unwrap();
            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                err: err.to_string(),
            });
        }

        tracing::debug!("Mod sync completed successfully");
        Ok(())
    }

    async fn execute_single_factorio_benchmark(&self, save_file: &Path) -> Result<String> {
        let mut cmd = self.factorio.create_command();

        cmd.args([
            "--benchmark",
            save_file
                .to_str()
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: save_file.to_path_buf(),
                })?,
            "--benchmark-ticks",
            &self.config.ticks.to_string(),
            "--benchmark-runs",
            "1", // Always run single benchmark
            "--disable-audio",
        ]);

        if let Some(mods_file) = &self.config.mods_dir {
            cmd.arg("--mod-directory");
            cmd.arg(
                mods_file
                    .to_str()
                    .ok_or_else(|| BenchmarkError::InvalidModsFileName {
                        path: mods_file.clone(),
                    })?,
            );
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;

        if !output.status.success() {
            let err = output.stdout.last().unwrap();
            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                err: err.to_string(),
            });
        }

        let stdout =
            String::from_utf8(output.stdout).map_err(|_| BenchmarkError::InvalidUtf8Output)?;

        Ok(stdout)
    }
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m{secs}s")
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        format!("{hours}h{mins}m")
    }
}
