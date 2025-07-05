//! Running and collecting logs of benchmarks on save file(s)

use charming::ImageRenderer;
use charming::theme::Theme;
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

/// A job, indicating a single benchmark run, to be used in queues of a specific order
#[derive(Debug, Clone)]
struct ExecutionJob {
    save_file: PathBuf,
    run_index: usize,
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
    pub async fn run_all(&self, save_files: Vec<PathBuf>) -> Result<Vec<BenchmarkResult>> {
        let execution_schedule = self.create_execution_schedule(&save_files);
        let total_jobs = execution_schedule.len();
        let start_time = Instant::now();
        let mut parsed_version = String::new();

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

            // Run a single benchmark and get the run data and version
            let (run, version) = self.run_single_benchmark(job).await?;
            parsed_version = version;

            if let Some(runs) = results_map.get_mut(&save_name) {
                runs.push(run);
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
                        factorio_version: parsed_version.clone(),
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
    async fn run_single_benchmark(&self, job: &ExecutionJob) -> Result<(BenchmarkRun, String)> {
        // If mods_file is not set, sync mods with the given save file
        if self.config.mods_dir.is_none() {
            self.sync_mods_for_save(&job.save_file).await?;
        }

        let factorio_output = self
            .execute_single_factorio_benchmark(&job.save_file)
            .await?;

        if self.config.verbose_charts {
            if let Some(verbose_data) = &factorio_output.verbose_data {
                let save_name = &job.save_file.file_stem().unwrap().to_string_lossy();
                let title = format!(
                    "wholeUpdate per Tick for {} - Run {}",
                    save_name,
                    &job.run_index + 1
                );

                match crate::benchmark::charts::generate_verbose_chart(verbose_data, &title) {
                    Ok(chart) => {
                        let chart_path = self
                            .config
                            .output
                            .as_deref()
                            .unwrap_or(Path::new("."))
                            .join(format!(
                                "{}_run{}_verbose.svg",
                                save_name,
                                &job.run_index + 1
                            ));

                        let mut renderer = ImageRenderer::new(1000, 1000).theme(Theme::Walden);
                        if let Err(e) = renderer.save(&chart, &chart_path) {
                            tracing::error!("Failed to save verbose chart: {e}");
                        } else {
                            tracing::info!("Verbose chart saved to {}", chart_path.display());
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse verbose data for chart: {e}");
                    }
                }
            }
        }

        let result = parser::parse_benchmark_log(&factorio_output, &job.save_file, &self.config)
            .map_err(|e| BenchmarkError::ParseError {
                reason: format!("Failed to parse benchmark log: {e}"),
            })?;

        // Extract the single run (since we're only running 1 benchmark at a time)
        if result.runs.len() != 1 {
            return Err(BenchmarkError::ParseError {
                reason: format!("Expected 1 run, got {}", result.runs.len()),
            });
        }

        // Return the run and the version, preserving the parsed version.
        let run = result.runs.into_iter().next().unwrap();
        Ok((run, result.factorio_version))
    }

    /// Sync Factorio's mods to the given save
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

            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                hint,
            });
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
                .ok_or_else(|| BenchmarkError::InvalidSaveFileName {
                    path: save_file.to_path_buf(),
                })?,
            "--benchmark-ticks",
            &self.config.ticks.to_string(),
            "--benchmark-runs",
            "1", // Always run single benchmark
            "--disable-audio",
        ]);

        if self.config.verbose_charts {
            cmd.arg("--benchmark-verbose");
            cmd.arg("all");
        }

        // Run with the argument --mod-directory if a mod-directory was given
        if let Some(mods_dir) = &self.config.mods_dir {
            cmd.arg("--mod-directory");
            cmd.arg(
                mods_dir
                    .to_str()
                    .ok_or_else(|| BenchmarkError::InvalidModsFileName {
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

            return Err(BenchmarkError::FactorioProcessFailed {
                code: output.status.code().unwrap_or(-1),
                hint,
            });
        }

        let stdout =
            String::from_utf8(output.stdout).map_err(|_| BenchmarkError::InvalidUtf8Output)?;
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

/// Helper function to turn a Duration into a nicely formatted string
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
