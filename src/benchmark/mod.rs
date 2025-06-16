pub mod discovery;
pub mod runner;
pub mod results;
pub mod parser;

use std::path::PathBuf;

use crate::core::{GlobalConfig, FactorioExecutor};

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub saves_dir: PathBuf,
    pub ticks: u32,
    pub runs: u32,
    pub pattern: Option<String>,
    pub output: PathBuf,
}

pub async fn run(global_config: GlobalConfig, benchmark_config: BenchmarkConfig) -> anyhow::Result<()> {
    tracing::info!("Starting benchmark with config: {:?}", benchmark_config);
    
    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!("Using Factorio at: {}", factorio.executable_path().display());

    let save_files = discovery::find_save_files(&benchmark_config.saves_dir, benchmark_config.pattern.as_deref())?;
    discovery::validate_save_files(&save_files)?;

    let runner = runner::BenchmarkRunner::new(benchmark_config.clone(), factorio);
    let results = runner.run_all(save_files).await?;

    // results::save_to_csv(&results, &benchmark_config.output)?;

    tracing::info!("Benchmark complete! Results saved to: {}", benchmark_config.output.display());
    tracing::info!("Total benchmarks run: {}", results.len());

    Ok(())
}
