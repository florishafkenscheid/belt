//! Main binary entrypoint for the BELT benchmarking tool.
//!
//! Parses CLI arguments, sets up logging, and dispatches to subcommands.

mod benchmark;
mod blueprint;
mod core;
mod sanitize;

use crate::core::{
    GlobalConfig, Result, RunOrder,
    config::{self, BenchmarkConfig, BlueprintConfig, SanitizeConfig},
    error::BenchmarkErrorKind,
};
use clap::{CommandFactory, Parser, Subcommand};
use std::{
    path::{Path, PathBuf},
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
    command: Option<Commands>,

    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Path to Factorio executable"
    )]
    factorio_path: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Enable verbose logging"
    )]
    verbose: bool,

    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Path to config file (default: ~/.config/belt/config.toml)"
    )]
    config: Option<PathBuf>,

    #[arg(
        long,
        help_heading = "Global Options",
        help = "Initialize config directory with example config"
    )]
    init_config: bool,

    #[arg(
        long,
        help_heading = "Global Options",
        help = "Prints the BELT binary's version"
    )]
    version: bool,

    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Run Factorio in headless mode"
    )]
    headless: bool,
}

#[derive(Subcommand)]
enum Commands {
    #[command(next_help_heading = "Benchmark Options")]
    Benchmark {
        /// Directory containing save files to benchmark
        #[arg(value_name = "SAVES_DIR")]
        saves_dir: Option<PathBuf>,

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
            help = "Export per-tick CSV data for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). Use 'all' to export all metrics."
        )]
        verbose_metrics: Option<Vec<String>>,

        #[arg(long, help = "Prefix to strip from save file names in output")]
        strip_prefix: Option<String>,

        #[arg(long, help = "Record CPU frequency data during benchmark runs")]
        record_cpu: bool,

        #[arg(
            long,
            help = "Append the results of this benchmark to existing belt data as specified by --output",
            long_help = "Append benchmark rows to existing output CSV files. Existing CSV headers must match the current output format and selected verbose metrics. Reports are regenerated from available CSV data, so details not stored in results.csv may not be preserved."
        )]
        append: bool,
    },
    #[command(next_help_heading = "Blueprint Options")]
    Blueprint {
        /// Directory containing blueprint files
        blueprints_dir: PathBuf,

        /// Path to the base save file for blueprint testing
        base_save_path: PathBuf,

        #[arg(long, help = "Number of blueprints to test")]
        count: Option<u32>,

        #[arg(long, help = "Number of buffer ticks before measuring")]
        buffer_ticks: Option<u32>,

        #[arg(long, default_value = "speed-module-3")]
        mining_module_replacement: String,

        #[arg(long, default_value = "legendary")]
        mining_module_replacement_quality: String,

        #[arg(long, help = "Directory containing mods to use")]
        mods_dir: Option<PathBuf>,

        #[arg(long, help = "Prefix for output file names")]
        prefix: Option<String>,

        #[arg(long, help = "Pattern to filter blueprint files")]
        pattern: Option<String>,

        #[arg(long, help = "Output directory or file path")]
        output: Option<PathBuf>,

        #[arg(long, help = "Number of construction bots to use")]
        bot_count: Option<u32>,
    },
    #[command(next_help_heading = "Sanitize Options")]
    Sanitize {
        /// Directory containing save files to sanitize
        #[arg(value_name = "SAVES_DIR")]
        saves_dir: Option<PathBuf>,

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
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse input
    let cli = Cli::parse();

    if cli.init_config {
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

    if cli.version {
        let version = env!("CARGO_PKG_VERSION");
        let bin = env!("CARGO_BIN_NAME");
        println!("{bin} v{version}");

        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

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
        &command,
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
    let result = match command {
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
            record_cpu,
            append,
        } => {
            async {
                let mut benchmark_config =
                    BenchmarkConfig::from_figment(&figment).unwrap_or_default();
                benchmark_config.append = append;

                if let Some(v) = saves_dir {
                    benchmark_config.saves_dir = v;
                }
                require_saves_dir(&benchmark_config.saves_dir, "benchmark")?;

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
                if cli.headless {
                    benchmark_config.headless = true;
                }
                if record_cpu {
                    benchmark_config.record_cpu = true;
                }

                benchmark::run(global_config, benchmark_config, &running).await
            }
            .await
        }

        Commands::Blueprint {
            blueprints_dir,
            base_save_path,
            count,
            buffer_ticks,
            mining_module_replacement,
            mining_module_replacement_quality,
            mods_dir,
            pattern,
            output,
            prefix,
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
            blueprint_config.mining_module_replacement = mining_module_replacement;
            blueprint_config.mining_module_replacement_quality = mining_module_replacement_quality;
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
            if cli.headless {
                blueprint_config.headless = true;
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
        } => {
            async {
                let mut sanitize_config =
                    SanitizeConfig::from_figment(&figment).unwrap_or_default();
                if let Some(v) = saves_dir {
                    sanitize_config.saves_dir = v;
                }
                require_saves_dir(&sanitize_config.saves_dir, "sanitize")?;

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
                if cli.headless {
                    sanitize_config.headless = true;
                }
                sanitize::run(global_config, sanitize_config, &running).await
            }
            .await
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

fn require_saves_dir(saves_dir: &Path, section: &str) -> Result<()> {
    if saves_dir.as_os_str().is_empty() {
        return Err(BenchmarkErrorKind::ConfigLoadError(format!(
            "SAVES_DIR is required unless {section}.saves_dir is set in config"
        ))
        .into());
    }

    Ok(())
}
