//! Main binary entrypoint for the BELT benchmarking tool.
//!
//! Parses CLI arguments, sets up logging, and dispatches to subcommands.

mod analyze;
mod benchmark;
mod core;
mod sanitize;

use crate::core::{
    Result, RunOrder,
    config::{AnalyzeConfig, BenchmarkConfig, SanitizeConfig},
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "belt")]
#[command(about = "Factorio benchmarking tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    factorio_path: Option<PathBuf>,

    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        data_dir: PathBuf,
    },
    Benchmark {
        saves_dir: PathBuf,

        #[arg(long, default_value = "6000")]
        ticks: u32,

        #[arg(long, default_value = "5")]
        runs: u32,

        #[arg(long)]
        pattern: Option<String>,

        #[arg(long)]
        output: Option<PathBuf>,

        #[arg(long)]
        template_path: Option<PathBuf>,

        #[arg(long)]
        mods_dir: Option<PathBuf>,

        #[arg(long, default_value = "grouped")]
        #[arg(
            help = "Execution order: sequential (A,B,A,B), random (A,B,B,A), or grouped (A,A,B,B)"
        )]
        run_order: RunOrder,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Generate per-tick charts for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). 'all' to chart all metrics."
        )]
        verbose_metrics: Vec<String>,

        #[arg(long)]
        strip_prefix: Option<String>,

        #[arg(
            long,
            default_value = "0",
            help = "Apply a simple moving average to per-tick data with the given window size. Set to 0 for no smoothing."
        )]
        smooth_window: u32,
    },
    Sanitize {},
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse input
    let cli = Cli::parse();

    // Toggle the tracing level
    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    // Create a global config for all subcommands
    let global_config = core::GlobalConfig {
        factorio_path: cli.factorio_path,
        verbose: cli.verbose,
    };

    // Capture the result of the benchmark
    let result = match cli.command {
        Commands::Analyze { data_dir } => {
            let analyze_config = AnalyzeConfig { data_dir };
            analyze::run(global_config, analyze_config).await
        }

        // Run the benchmark with a newly created benchmark config
        Commands::Benchmark {
            saves_dir,
            ticks,
            runs,
            pattern,
            output,
            template_path,
            mods_dir,
            run_order,
            verbose_metrics,
            strip_prefix,
            smooth_window,
        } => {
            let benchmark_config = BenchmarkConfig {
                saves_dir,
                ticks,
                runs,
                pattern,
                output,
                template_path,
                mods_dir,
                run_order,
                verbose_metrics,
                strip_prefix,
                smooth_window,
            };

            benchmark::run(global_config, benchmark_config).await
        }

        Commands::Sanitize {} => {
            let sanitize_config = SanitizeConfig {};
            sanitize::run(global_config, sanitize_config).await
        }
    };

    // If any command results in an error, print and exit
    if let Err(e) = result {
        tracing::error!("{e}");

        if let Some(hint_text) = e.get_hint() {
            tracing::error!("{hint_text}");
        }

        std::process::exit(1);
    }

    Ok(())
}
