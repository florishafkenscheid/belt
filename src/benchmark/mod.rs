//! Benchmarking module
//!
//! Contains logic for running, parsing, and reporting Factorio benchmarks.

pub mod parser;
pub mod runner;

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use crate::{
    benchmark::runner::VerboseData,
    core::{
        FactorioExecutor, GlobalConfig, Result,
        config::BenchmarkConfig,
        output::{CsvWriter, ResultWriter, WriteData, ensure_output_dir, report::ReportWriter},
        utils,
    },
};

/// Run all of the benchmarks, capture the logs and write the results to files.
pub async fn run(
    global_config: GlobalConfig,
    benchmark_config: BenchmarkConfig,
    running: &Arc<AtomicBool>,
) -> Result<()> {
    tracing::debug!("Starting benchmark with config: {:?}", benchmark_config);

    // Find the Factorio binary
    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!(
        "Using Factorio at: {}",
        factorio.executable_path().display()
    );

    // Find the specified save files
    let save_files = utils::find_save_files(
        &benchmark_config.saves_dir,
        benchmark_config.pattern.as_deref(),
    )?;
    // Validate the found save files
    utils::validate_save_files(&save_files)?;

    let output_dir = benchmark_config
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    ensure_output_dir(output_dir)?;
    tracing::debug!("Output directory: {}", output_dir.display());

    // Run the benchmarks
    let runner = runner::BenchmarkRunner::new(benchmark_config.clone(), factorio);
    let (mut results, all_runs_verbose_data) = runner.run_all(save_files, running).await?;
    // Calculate the percentage difference from the worst performer
    utils::calculate_base_differences(&mut results);

    if !benchmark_config.verbose_metrics.is_empty() && !all_runs_verbose_data.is_empty() {
        // Group verbose data by save
        let mut verbose_data_by_save: HashMap<String, Vec<VerboseData>> = HashMap::new();
        for data in all_runs_verbose_data {
            verbose_data_by_save
                .entry(data.save_name.clone())
                .or_default()
                .push(data);
        }

        // First, write all CSV files to ensure they're created before potentially memory-intensive chart operations
        let csv_writer = CsvWriter::new();
        for (save_name, save_verbose_data) in &verbose_data_by_save {
            let data = WriteData::Verbose {
                data: save_verbose_data.to_vec(),
                metrics_to_export: benchmark_config.verbose_metrics.clone(),
            };
            csv_writer.write(&data, output_dir)?;
            tracing::debug!("Wrote verbose metrics for save: {save_name}");
        }
    }

    // Write the csv's
    let csv_writer = CsvWriter::new();
    let data = WriteData::Benchmark(results.clone());
    csv_writer.write(&data, output_dir)?;

    // Write the report
    let report_writer = ReportWriter::new();
    let data = WriteData::Report {
        data: results.clone(),
        template_path: benchmark_config.template_path.as_deref(),
    };
    report_writer.write(&data, output_dir)?;

    tracing::info!("Benchmark complete!");
    tracing::info!("Total benchmarks run: {}", results.len());

    Ok(())
}
