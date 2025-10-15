//! Running and collecting logs of benchmarks on save file(s)

use indicatif::{ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;

use super::BenchmarkConfig;
use crate::benchmark::parser;
use crate::benchmark::parser::BenchmarkResult;
use crate::core::Result;
use crate::core::error::BenchmarkErrorKind;
use crate::core::factorio::FactorioTickRunSpec;
use crate::core::format_duration;
use crate::core::{FactorioExecutor, RunOrder};

/// A job, indicating a single benchmark run, to be used in queues of a specific order
#[derive(Debug, Clone)]
struct ExecutionJob {
    save_file: PathBuf,
    run_index: usize,
}

#[derive(Debug, Clone)]
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
        running: &Arc<AtomicBool>,
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
            if !running.load(Ordering::SeqCst) {
                tracing::info!("Shutdown requested. Aborting remaining benchmarks.");
                break;
            }

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

            // Run a single benchmark and get the run data and version
            let (mut result_for_run, verbose_data) = self.run_single_benchmark(job).await?;

            if let Some(existing_result) = results_map.get_mut(&result_for_run.save_name) {
                existing_result.runs.append(&mut result_for_run.runs);
            } else {
                results_map.insert(result_for_run.save_name.clone(), result_for_run);
            }

            if let Some(data) = verbose_data {
                all_verbose_data.push(data);
            }
        }

        if !running.load(Ordering::SeqCst) {
            progress.finish_with_message("Benchmarking interrupted.");
        } else {
            progress.finish_with_message("Benchmarking complete!");
        }

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
            self.factorio.sync_mods_for_save(&job.save_file).await?;
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

    /// Execute a single factorio benchmark run
    async fn execute_single_factorio_benchmark(&self, save_file: &Path) -> Result<FactorioOutput> {
        self.factorio
            .run_for_ticks(FactorioTickRunSpec {
                save_file,
                ticks: self.config.ticks,
                mods_dir: self.config.mods_dir.as_deref(),
                verbose_all_metrics: !self.config.verbose_metrics.is_empty(),
                headless: self.config.headless,
            })
            .await
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
