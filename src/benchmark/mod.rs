//! Benchmarking module
//!
//! Contains logic for running, parsing, and reporting Factorio benchmarks.

pub mod charts;
pub mod discovery;
pub mod parser;
pub mod runner;

use std::path::{Path, PathBuf};

use charming::{ImageRenderer, theme::Theme};

use crate::core::{BenchmarkError, FactorioExecutor, GlobalConfig, Result, output};

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
            _ => Err(BenchmarkError::InvalidRunOrder {
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
    std::fs::create_dir_all(output_dir).map_err(|_| BenchmarkError::DirectoryCreationFailed {
        path: output_dir.to_path_buf(),
    })?;
    tracing::debug!("Output directory: {}", output_dir.display());

    // Run the benchmarks
    let runner = runner::BenchmarkRunner::new(benchmark_config.clone(), factorio);
    let (mut results, verbose_data) = runner.run_all(save_files).await?;
    // Calculate the percentage difference from the worst performer
    parser::calculate_base_differences(&mut results);

    let mut renderer = ImageRenderer::new(1000, 1000).theme(Theme::Walden);

    if !benchmark_config.verbose_metrics.is_empty() && !verbose_data.is_empty() {
        tracing::info!("Generating per-tick charts for requested metrics...");

        for data in &verbose_data {
            match charts::create_verbose_charts_for_metrics(
                &data.csv_data,
                &data.save_name,
                data.run_index,
                &benchmark_config.verbose_metrics,
            ) {
                Ok(charts_with_names) => {
                    for (chart, metric_name) in charts_with_names {
                        let chart_path = output_dir.join(format!(
                            "{}_run{}_{}_verbose.svg",
                            data.save_name,
                            data.run_index + 1,
                            metric_name
                        ));
                        if let Err(e) = renderer.save(&chart, &chart_path) {
                            tracing::error!(
                                "Failed to save verbose chart for {} (metric: {}): {}",
                                data.save_name,
                                metric_name,
                                e
                            );
                        } else {
                            tracing::info!(
                                "Verbose chart for {} (metric: {}) saved to {}",
                                data.save_name,
                                metric_name,
                                chart_path.display()
                            );
                        }
                    }
                }
                Err(e) => tracing::error!(
                    "Failed to create verbose charts for {} (run {}): {}",
                    data.save_name,
                    data.run_index + 1,
                    e
                ),
            }
        }
    }

    // Capture specified, or use a default template file
    let template_path = benchmark_config
        .template_path
        .as_deref()
        .unwrap_or_else(|| Path::new("templates/benchmark.md.hbs"));

    // Write the results to the csv and md files
    output::write_results(&results, output_dir, template_path, &mut renderer).await?;

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
