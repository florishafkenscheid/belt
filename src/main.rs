//! Main binary entrypoint for the BELT benchmarking tool.
//!
//! Parses CLI arguments, sets up logging, and dispatches to subcommands.

mod analyze;
mod benchmark;
mod blueprint;
mod core;
mod sanitize;

use crate::core::{
    GlobalConfig, Result, RunOrder,
    config::{self, AnalyzeConfig, BenchmarkConfig, BlueprintConfig, SanitizeConfig},
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

    #[arg(long, global = true, help = "Path to Factorio executable")]
    factorio_path: Option<PathBuf>,

    #[arg(long, global = true, help = "Enable verbose logging")]
    verbose: bool,

    #[arg(
        long,
        global = true,
        help = "Path to config file (default: ~/.config/belt/config.toml)"
    )]
    config: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help = "Initialize config directory with example config"
    )]
    init_config: bool,
}

#[derive(Subcommand)]
enum Commands {
    Analyze {
        /// Directory containing benchmark data files
        data_dir: PathBuf,

        #[arg(
            long,
            help = "Apply a simple moving average to per-tick data with the given window size. Set to 0 for no smoothing."
        )]
        smooth_window: Option<u32>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Generate per-tick charts for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). 'all' to chart all metrics."
        )]
        verbose_metrics: Option<Vec<String>>,

        #[arg(long, help = "Chart height in pixels")]
        height: Option<u32>,

        #[arg(long, help = "Chart width in pixels")]
        width: Option<u32>,

        #[arg(
            long,
            help = "Max data points that the verbose charts can reach before being downsampled."
        )]
        max_points: Option<usize>,
    },
    Benchmark {
        /// Directory containing save files to benchmark
        saves_dir: PathBuf,

        #[arg(long, help = "Number of ticks to run each benchmark")]
        ticks: Option<u32>,

        #[arg(long, help = "Number of benchmark runs per save file")]
        runs: Option<u32>,

        #[arg(long, help = "Pattern to filter save files")]
        pattern: Option<String>,

        #[arg(long, help = "Output directory or file path")]
        output: Option<PathBuf>,

        #[arg(long, help = "Path to handlebars report template")]
        template_path: Option<PathBuf>,

        #[arg(long, help = "Directory containing mods to use")]
        mods_dir: Option<PathBuf>,

        #[arg(
            long,
            help = "Execution order: sequential (A,B,A,B), random (A,B,B,A), or grouped (A,A,B,B)"
        )]
        run_order: Option<RunOrder>,

        #[arg(
            long,
            value_delimiter = ',',
            help = "Generate per-tick charts for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). 'all' to chart all metrics."
        )]
        verbose_metrics: Option<Vec<String>>,

        #[arg(long, help = "Prefix to strip from save file names in output")]
        strip_prefix: Option<String>,

        #[arg(long, help = "Run Factorio in headless mode")]
        headless: Option<bool>,
    },
    Blueprint {
        /// Directory containing blueprint files
        blueprints_dir: PathBuf,

        /// Path to the base save file for blueprint testing
        base_save_path: PathBuf,

        #[arg(long, help = "Number of blueprints to test")]
        count: Option<u32>,

        #[arg(long, help = "Number of buffer ticks before measuring")]
        buffer_ticks: Option<u32>,

        #[arg(long, help = "Directory containing mods to use")]
        mods_dir: Option<PathBuf>,

        #[arg(long, help = "Prefix for output file names")]
        prefix: Option<String>,

        #[arg(long, help = "Pattern to filter blueprint files")]
        pattern: Option<String>,

        #[arg(long, help = "Output directory or file path")]
        output: Option<PathBuf>,

        #[arg(long, help = "Run Factorio in headless mode")]
        headless: Option<bool>,

        #[arg(long, help = "Number of construction bots to use")]
        bot_count: Option<u32>,
    },
    Sanitize {
        /// Directory containing save files to sanitize
        saves_dir: PathBuf,

        #[arg(long, help = "Pattern to filter save files")]
        pattern: Option<String>,

        #[arg(long, help = "Number of ticks to run sanitization")]
        ticks: Option<u32>,

        #[arg(long, help = "Directory containing mods to use")]
        mods_dir: Option<PathBuf>,

        #[arg(long, help = "Output directory for sanitized saves")]
        data_dir: Option<PathBuf>,

        #[arg(long, help = "Items to preserve during sanitization (comma-separated)")]
        items: Option<String>,

        #[arg(
            long,
            help = "Fluids to preserve during sanitization (comma-separated)"
        )]
        fluids: Option<String>,

        #[arg(long, help = "Run Factorio in headless mode")]
        headless: Option<bool>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --init-config before CLI parsing (since subcommand is required)
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--init-config".to_string()) {
        match config::init_config_dir() {
            Ok(path) => {
                println!("Initialized config directory at: {}", path.display());
                return Ok(());
            }
            Err(e) => {
                eprintln!("Failed to initialize config directory: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Parse input
    let cli = Cli::parse();

    // Create figment from config file and environment variables
    let figment = if let Some(config_path) = &cli.config {
        match config::create_figment_from_file(config_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to load config file: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        config::create_figment().unwrap_or_else(|_| {
            // If figment creation fails, use empty figment (defaults only)
            figment::Figment::new()
        })
    };

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

    // Build global config: config file -> env vars -> CLI args
    let mut global_config = GlobalConfig::from_figment(&figment).unwrap_or_default();
    if cli.factorio_path.is_some() {
        global_config.factorio_path = cli.factorio_path;
    }
    if cli.verbose {
        global_config.verbose = cli.verbose;
    }

    // Listen to CTRL+C
    let needs_shutdown = matches!(
        cli.command,
        Commands::Benchmark { .. } | Commands::Sanitize { .. } | Commands::Blueprint { .. }
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
            // Load config from file, then override with CLI args
            let mut analyze_config = AnalyzeConfig::from_figment(&figment).unwrap_or_default();
            analyze_config.data_dir = data_dir;
            if let Some(v) = smooth_window {
                analyze_config.smooth_window = v;
            }
            if let Some(v) = verbose_metrics {
                analyze_config.verbose_metrics = v;
            }
            if let Some(v) = height {
                analyze_config.height = v;
            }
            if let Some(v) = width {
                analyze_config.width = v;
            }
            if let Some(v) = max_points {
                analyze_config.max_points = Some(v);
            }
            analyze::run(global_config, analyze_config).await
        }

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
            let mut benchmark_config = BenchmarkConfig::from_figment(&figment).unwrap_or_default();
            benchmark_config.saves_dir = saves_dir;
            if let Some(v) = ticks {
                benchmark_config.ticks = v;
            }
            if let Some(v) = runs {
                benchmark_config.runs = v;
            }
            if let Some(v) = pattern {
                benchmark_config.pattern = Some(v);
            }
            if let Some(v) = output {
                benchmark_config.output = Some(v);
            }
            if let Some(v) = template_path {
                benchmark_config.template_path = Some(v);
            }
            if let Some(v) = mods_dir {
                benchmark_config.mods_dir = Some(v);
            }
            if let Some(v) = run_order {
                benchmark_config.run_order = v;
            }
            if let Some(v) = verbose_metrics {
                benchmark_config.verbose_metrics = v;
            }
            if let Some(v) = strip_prefix {
                benchmark_config.strip_prefix = Some(v);
            }
            if let Some(v) = headless {
                benchmark_config.headless = Some(v);
            }
            benchmark::run(global_config, benchmark_config, &running).await
        }

        Commands::Blueprint {
            blueprints_dir,
            base_save_path,
            count,
            buffer_ticks,
            mods_dir,
            pattern,
            output,
            prefix,
            headless,
            bot_count,
        } => {
            let mut blueprint_config = BlueprintConfig::from_figment(&figment).unwrap_or_default();
            blueprint_config.blueprints_dir = blueprints_dir;
            blueprint_config.base_save_path = base_save_path;
            if let Some(v) = count {
                blueprint_config.count = v;
            }
            if let Some(v) = buffer_ticks {
                blueprint_config.buffer_ticks = v;
            }
            if let Some(v) = mods_dir {
                blueprint_config.mods_dir = Some(v);
            }
            if let Some(v) = pattern {
                blueprint_config.pattern = Some(v);
            }
            if let Some(v) = output {
                blueprint_config.output = Some(v);
            }
            if let Some(v) = prefix {
                blueprint_config.prefix = Some(v);
            }
            if let Some(v) = headless {
                blueprint_config.headless = Some(v);
            }
            if let Some(v) = bot_count {
                blueprint_config.bot_count = Some(v);
            }
            blueprint::run(global_config, blueprint_config, &running).await
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
            let mut sanitize_config = SanitizeConfig::from_figment(&figment).unwrap_or_default();
            sanitize_config.saves_dir = saves_dir;
            if let Some(v) = pattern {
                sanitize_config.pattern = Some(v);
            }
            if let Some(v) = ticks {
                sanitize_config.ticks = v;
            }
            if let Some(v) = mods_dir {
                sanitize_config.mods_dir = Some(v);
            }
            if let Some(v) = data_dir {
                sanitize_config.data_dir = Some(v);
            }
            if let Some(v) = items {
                sanitize_config.items = Some(v);
            }
            if let Some(v) = fluids {
                sanitize_config.fluids = Some(v);
            }
            if let Some(v) = headless {
                sanitize_config.headless = Some(v);
            }
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
