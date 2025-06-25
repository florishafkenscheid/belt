pub mod charts;
pub mod discovery;
pub mod parser;
pub mod runner;

use std::path::{Path, PathBuf};

use crate::core::{FactorioExecutor, GlobalConfig, output};

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub saves_dir: PathBuf,
    pub ticks: u32,
    pub runs: u32,
    pub pattern: Option<String>,
    pub output: Option<PathBuf>,
    pub template_path: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
}

pub async fn run(
    global_config: GlobalConfig,
    benchmark_config: BenchmarkConfig,
) -> anyhow::Result<()> {
    tracing::info!("Starting benchmark with config: {:?}", benchmark_config);

    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!(
        "Using Factorio at: {}",
        factorio.executable_path().display()
    );

    let save_files = discovery::find_save_files(
        &benchmark_config.saves_dir,
        benchmark_config.pattern.as_deref(),
    )?;
    discovery::validate_save_files(&save_files)?;

    let runner = runner::BenchmarkRunner::new(benchmark_config.clone(), factorio);
    let mut results = runner.run_all(save_files).await?;
    parser::calculate_base_differences(&mut results);

    let output_dir = benchmark_config
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new("."));

    tracing::debug!("Output directory: {}", output_dir.display());

    let template_path = benchmark_config
        .template_path
        .as_deref()
        .unwrap_or_else(|| Path::new("templates/benchmark.md.hbs"));
    
    output::write_results(&results, output_dir, template_path)?;

    tracing::info!("Benchmark complete!");
    tracing::info!("Total benchmarks run: {}", results.len());

    Ok(())
}
