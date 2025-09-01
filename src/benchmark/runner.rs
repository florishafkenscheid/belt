//! Running and collecting logs of benchmarks on save file(s)

use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::time::Instant;

use super::{BenchmarkConfig, RunOrder};
use crate::benchmark::discovery::check_sanitizer;
use crate::benchmark::parser;
use crate::benchmark::parser::BenchmarkResult;
use crate::core::FactorioExecutor;
use crate::core::Result;
use crate::core::error::BenchmarkErrorKind;
use crate::core::{BenchmarkError, format_duration};

/// A job, indicating a single benchmark run, to be used in queues of a specific order
#[derive(Debug, Clone)]
struct ExecutionJob {
    save_file: PathBuf,
    run_index: usize,
}

#[derive(Clone)]
pub struct VerboseData {
    pub save_name: String,
    pub run_index: usize,
    pub csv_data: String,
}

pub struct FactorioOutput {
    pub summary: String,
    pub verbose_data: Option<String>,
}

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    factorio: FactorioExecutor,
}

/// Runs the benchmarks, keeps a progress bar updated and returns results.
impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    /// Run benchmarks for all save files
    pub async fn run_all(
        &self,
        save_files: Vec<PathBuf>,
    ) -> Result<(Vec<BenchmarkResult>, Vec<VerboseData>)> {
        let execution_schedule = self.create_execution_schedule(&save_files);
        let total_jobs = execution_schedule.len();
        let start_time = Instant::now();
        let mut all_verbose_data: Vec<VerboseData> = Vec::new();
        let mut results_map: HashMap<String, BenchmarkResult> = HashMap::new();

        let progress = ProgressBar::new(total_jobs as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )?
            .progress_chars("=="),
        );
        progress.enable_steady_tick(Duration::from_millis(100));

        // Execute jobs according to schedule
        for (job_index, job) in execution_schedule.iter().enumerate() {
            let save_name = job
                .save_file
                .file_stem()
                .ok_or_else(|| BenchmarkErrorKind::InvalidSaveFileName {
                    path: job.save_file.clone(),
                })?
                .to_string_lossy()
                .to_string();

            let save_name = match self.config.strip_prefix.as_deref() {
                Some(prefix) => save_name
                    .strip_prefix(prefix)
                    .unwrap_or(&save_name)
                    .to_string(),
                None => save_name,
            };

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

            // Delete potential stale belt-sanitizer info
            let sanitizer_path = check_sanitizer();
            tracing::debug!("Attempting to delete sanitizer path: {:?}", sanitizer_path);
            match sanitizer_path {
                Some(path) => fs::remove_dir_all(path)?,
                None => tracing::debug!("No sanitizer from past run found."),
            }

            // Run a single benchmark and get the run data and version
            let (mut result_for_run, verbose_data) = self.run_single_benchmark(job).await?;

            let sanitizer_path = check_sanitizer();
            tracing::debug!("Attempting to parse sanitizer path: {:?}", sanitizer_path);
            match sanitizer_path {
                Some(path) => {
                    parser::parse_sanitizer(&result_for_run, &path)?;
                },
                None => {
                    tracing::debug!("No sanitizer found for save: {}", &result_for_run.save_name);
                }
            }

            if let Some(existing_result) = results_map.get_mut(&result_for_run.save_name) {
                existing_result.runs.append(&mut result_for_run.runs);
            } else {
                results_map.insert(result_for_run.save_name.clone(), result_for_run);
            }

            if let Some(data) = verbose_data {
                all_verbose_data.push(data);
            }
        }

        progress.finish_with_message("Benchmarking complete!");

        let mut all_results: Vec<BenchmarkResult> = results_map.into_values().collect();

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

        Ok((all_results, all_verbose_data))
    }

    /// Create the execution schedule based on the RunOrder
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

    /// Returns the benchmark run and the parsed Factorio version string
    async fn run_single_benchmark(
        &self,
        job: &ExecutionJob,
    ) -> Result<(BenchmarkResult, Option<VerboseData>)> {
        // If mods_file is not set, sync mods with the given save file
        if self.config.mods_dir.is_none() {
            self.sync_mods_for_save(&job.save_file).await?;
        }

        let factorio_output = self
            .execute_single_factorio_benchmark(&job.save_file)
            .await?;

        let verbose_data_for_return = if !self.config.verbose_metrics.is_empty() {
            factorio_output.verbose_data.map(|csv_data| VerboseData {
                save_name: job
                    .save_file
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                run_index: job.run_index,
                csv_data,
            })
        } else {
            None
        };

        let result =
            parser::parse_benchmark_log(&factorio_output.summary, &job.save_file, &self.config)?;

        // Extract the single run (since we're only running 1 benchmark at a time)
        if result.runs.len() != 1 {
            return Err(BenchmarkErrorKind::ParseError {
                reason: format!("Expected 1 run, got {}", result.runs.len()),
            }
            .into());
        }

        Ok((result, verbose_data_for_return))
    }

    /// Sync Factorio's mods to the given save
    async fn sync_mods_for_save(&self, save_file: &Path) -> Result<()> {
        let mut cmd = self.factorio.create_command();

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

    /// Execute a single factorio benchmark run
    async fn execute_single_factorio_benchmark(&self, save_file: &Path) -> Result<FactorioOutput> {
        let mut cmd = self.factorio.create_command();

        cmd.args([
            "--benchmark",
            save_file
                .to_str()
                .ok_or_else(|| BenchmarkErrorKind::InvalidSaveFileName {
                    path: save_file.to_path_buf(),
                })?,
            "--benchmark-ticks",
            &self.config.ticks.to_string(),
            "--benchmark-runs",
            "1", // Always run single benchmark
            "--disable-audio",
        ]);

        if !self.config.verbose_metrics.is_empty() {
            cmd.arg("--benchmark-verbose");
            cmd.arg("all");
        }

        // Run with the argument --mod-directory if a mod-directory was given
        if let Some(mods_dir) = &self.config.mods_dir {
            cmd.arg("--mod-directory");
            cmd.arg(
                mods_dir
                    .to_str()
                    .ok_or_else(|| BenchmarkErrorKind::InvalidModsFileName {
                        path: mods_dir.clone(),
                    })?,
            );
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd.spawn()?.wait_with_output().await?;

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

#[cfg(test)]
mod tests {
    use crate::core::format_duration;

    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(Duration::from_secs(61)), "1m1s");
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h1m");
    }
}
