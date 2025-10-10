//! Main binary entrypoint for the BELT benchmarking tool.
//!
//! Parses CLI arguments, sets up logging, and dispatches to subcommands.

mod analyze;
mod benchmark;
mod core;
mod sanitize;

use crate::core::{
    GlobalConfig, Result, RunOrder,
    config::{AnalyzeConfig, BenchmarkConfig, SanitizeConfig},
};
use clap::{Parser, Subcommand};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

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

        #[arg(
            long,
            default_value = "0",
            help = "Apply a simple moving average to per-tick data with the given window size. Set to 0 for no smoothing."
        )]
        smooth_window: u32,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Generate per-tick charts for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). 'all' to chart all metrics."
        )]
        verbose_metrics: Vec<String>,

        #[arg(long)]
        height: u32,

        #[arg(long)]
        width: u32,

        #[arg(
            long,
            help = "Max data points that the verbose charts can reach before being downsampled."
        )]
        max_points: Option<usize>,
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

        #[arg(long)]
        headless: Option<bool>,
    },
    Sanitize {
        saves_dir: PathBuf,

        #[arg(long)]
        pattern: Option<String>,

        #[arg(long, default_value = "3600")]
        ticks: u32,

        #[arg(long)]
        mods_dir: Option<PathBuf>,

        #[arg(long)]
        data_dir: Option<PathBuf>,

        #[arg(long)]
        items: Option<String>,

        #[arg(long)]
        fluids: Option<String>,

        #[arg(long)]
        headless: Option<bool>,
    },
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

    let global_config = GlobalConfig {
        factorio_path: cli.factorio_path,
        verbose: cli.verbose,
    };

    // Listen to CTRL+C
    let needs_shutdown = matches!(
        cli.command,
        Commands::Benchmark { .. } | Commands::Sanitize { .. }
    );
    let running = Arc::new(AtomicBool::new(true));
    let shutdown_task = if needs_shutdown {
        let r = running.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::warn!("Failed to listen for CTRL+C: {e}");
            }
            tracing::info!("Received CTRL+C. Initiating graceful shutdown...");
            r.store(false, Ordering::SeqCst);
        }))
    } else {
        None
    };

    // Capture the result of the benchmark
    let result = match cli.command {
        Commands::Analyze {
            data_dir,
            smooth_window,
            verbose_metrics,
            height,
            width,
            max_points,
        } => {
            let analyze_config = AnalyzeConfig {
                data_dir,
                smooth_window,
                verbose_metrics,
                height,
                width,
                max_points,
            };
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
            headless,
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
                headless,
            };

            benchmark::run(global_config, benchmark_config, &running).await
        }

        Commands::Sanitize {
            saves_dir,
            pattern,
            ticks,
            mods_dir,
            data_dir,
            items,
            fluids,
            headless,
        } => {
            let sanitize_config = SanitizeConfig {
                saves_dir,
                pattern,
                ticks,
                mods_dir,
                data_dir,
                items,
                fluids,
                headless,
            };
            sanitize::run(global_config, sanitize_config, &running).await
        }
    };

    // Await shutdown if needed
    if let Some(task) = shutdown_task {
        let interrupted = !running.load(Ordering::SeqCst);
        if interrupted {
            let _ = task.await;
            tracing::info!("Shutdown complete");
        } else {
            drop(task);
        }
    }

    // If any command results in an error, print and exit
    if let Err(e) = result {
        tracing::error!("{e}");

        std::process::exit(1);
    }

    Ok(())
}
