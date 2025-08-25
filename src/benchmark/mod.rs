//! Benchmarking module
//!
//! Contains logic for running, parsing, and reporting Factorio benchmarks.

pub mod charts;
pub mod discovery;
pub mod parser;
pub mod runner;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use charming::{ImageRenderer, theme::Theme};

use crate::{
    benchmark::runner::VerboseData,
    core::{FactorioExecutor, GlobalConfig, Result, error::BenchmarkErrorKind, output, utils},
};

#[derive(Debug, Clone, Default)]
pub enum RunOrder {
    Sequential,
    Random,
    #[default]
    Grouped,
}

// Get a RunOrder from a string
impl std::str::FromStr for RunOrder {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sequential" => Ok(RunOrder::Sequential),
            "random" => Ok(RunOrder::Random),
            "grouped" => Ok(RunOrder::Grouped),
            _ => Err(BenchmarkErrorKind::InvalidRunOrder {
                input: s.to_string(),
            }
            .to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BenchmarkConfig {
    pub saves_dir: PathBuf,
    pub ticks: u32,
    pub runs: u32,
    pub pattern: Option<String>,
    pub output: Option<PathBuf>,
    pub template_path: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
    pub run_order: RunOrder,
    pub verbose_metrics: Vec<String>,
    pub strip_prefix: Option<String>,
    pub smooth_window: u32,
}

// Run all of the benchmarks, capture the logs and write the results to files.
pub async fn run(global_config: GlobalConfig, benchmark_config: BenchmarkConfig) -> Result<()> {
    tracing::info!("Starting benchmark with config: {:?}", benchmark_config);

    // Find the Factorio binary
    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!(
        "Using Factorio at: {}",
        factorio.executable_path().display()
    );

    // Find the specified save files
    let save_files = discovery::find_save_files(
        &benchmark_config.saves_dir,
        benchmark_config.pattern.as_deref(),
    )?;
    // Validate the found save files
    discovery::validate_save_files(&save_files)?;

    let output_dir = benchmark_config
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(output_dir)?;
    tracing::debug!("Output directory: {}", output_dir.display());

    // Run the benchmarks
    let runner = runner::BenchmarkRunner::new(benchmark_config.clone(), factorio);
    let (mut results, all_runs_verbose_data) = runner.run_all(save_files).await?;
    // Calculate the percentage difference from the worst performer
    utils::calculate_base_differences(&mut results);

    let mut renderer = ImageRenderer::new(1000, 1000).theme(Theme::Walden);

    if !benchmark_config.verbose_metrics.is_empty() && !all_runs_verbose_data.is_empty() {
        // Group verbose data by save
        let mut verbose_data_by_save: HashMap<String, Vec<VerboseData>> = HashMap::new();
        for data in all_runs_verbose_data {
            verbose_data_by_save
                .entry(data.save_name.clone())
                .or_default()
                .push(data);
        }

        let all_verbose_data: Vec<VerboseData> = verbose_data_by_save
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();

        // First, write all CSV files to ensure they're created before potentially memory-intensive chart operations
        for (save_name, save_verbose_data) in &verbose_data_by_save {
            output::write_verbose_metrics_csv(
                save_name,
                save_verbose_data,
                &benchmark_config.verbose_metrics,
                output_dir,
            )?;
        }

        tracing::info!("Generating per-tick charts for requested metrics...");
        let mut wide_renderer = ImageRenderer::new(2000, 1000).theme(Theme::Walden);

        const MAX_SAFE_POINTS: u64 = 1_000_000;
        
        let global_metric_bounds = charts::compute_global_metric_bounds(
            &all_verbose_data,
            &benchmark_config.verbose_metrics,
            benchmark_config.smooth_window,
        );
        
        // Then create charts, which could potentially cause OOM errors in echarts
        for (save_name, save_verbose_data) in verbose_data_by_save {
            let ticks_per_run = benchmark_config.ticks as u64;
            let num_runs = save_verbose_data.len() as u64;
            let total_points = ticks_per_run * num_runs;
            
            if total_points > MAX_SAFE_POINTS {
                tracing::warn!(
                    "Skipping per-tick charts for {} ({} points exceed safe limit of {}; consider increasing --smooth-window or reducing ticks/runs)",
                    save_name,
                    total_points,
                    MAX_SAFE_POINTS
                );
                continue;
            }
            
            match charts::create_all_verbose_charts_for_save(
                &save_name,
                &save_verbose_data,
                &benchmark_config.verbose_metrics,
                benchmark_config.smooth_window,
                &global_metric_bounds,
            ) {
                Ok(charts_with_names) => {
                    for (chart, metric_name) in charts_with_names {
                        let chart_path =
                            output_dir.join(format!("{save_name}_{metric_name}_per_tick.svg"));
                        if let Err(e) = wide_renderer.save(&chart, &chart_path) {
                            tracing::error!(
                                "Failed to save per-tick chart for {} (metric: {}): {}",
                                save_name,
                                metric_name,
                                e
                            );
                        } else {
                            tracing::info!(
                                "Per-tick chart for {} (metric: {}) saved to {}",
                                save_name,
                                metric_name,
                                chart_path.display()
                            );
                        }
                    }
                }
                Err(e) => tracing::error!(
                    "Failed to create per-tick charts for save {}: {}",
                    save_name,
                    e
                ),
            }
        }
    }

    // Write the results to the csv and md files
    output::write_results(
        &results,
        output_dir,
        benchmark_config.template_path,
        &mut renderer,
    )
    .await?;

    tracing::info!("Benchmark complete!");
    tracing::info!(
        "Total benchmarks run: {}",
        results
            .iter()
            .map(|result| result.runs.len() as u64)
            .sum::<u64>()
    );

    Ok(())
}
