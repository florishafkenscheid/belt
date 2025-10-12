//! Blueprint Benchmarking module
//!
//! Contains logic for running blueprints, then uses the normal benchmark stuff to report results.

pub mod runner;

use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use crate::
    core::{
        FactorioExecutor, GlobalConfig, Result,
        config::BlueprintBenchmarkConfig,
        output::ensure_output_dir,
        utils,
    }
;

/// Run all of the benchmarks, capture the logs and write the results to files.
pub async fn run(
    global_config: GlobalConfig,
    benchmark_config: BlueprintBenchmarkConfig,
    running: &Arc<AtomicBool>,
) -> Result<()> {
    tracing::info!("Starting blueprint benchmark with config: {:?}", benchmark_config);

    // Find the Factorio binary
    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!(
        "Using Factorio at: {}",
        factorio.executable_path().display()
    );

    // Find the specified blueprint files
    let blueprint_files = utils::find_blueprint_files(
        &benchmark_config.blueprints_dir,
        benchmark_config.pattern.as_deref(),
    )?;

    let output_dir = benchmark_config
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new("."));
    ensure_output_dir(output_dir)?;
    tracing::debug!("Output directory: {}", output_dir.display());

    // Run the benchmarks
    let runner = runner::BlueprintBenchmarkRunner::new(benchmark_config.clone(), factorio);
    let (_, _) = runner.run_all(blueprint_files, running).await?;
    
    Ok(())
}
