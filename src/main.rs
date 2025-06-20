mod benchmark;
mod core;

use anyhow::Result;
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
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    let global_config = core::GlobalConfig {
        factorio_path: cli.factorio_path,
        verbose: cli.verbose,
    };

    match cli.command {
        Commands::Benchmark {
            saves_dir,
            ticks,
            runs,
            pattern,
            output,
            template_path,
            mods_dir,
        } => {
            let benchmark_config = benchmark::BenchmarkConfig {
                saves_dir,
                ticks,
                runs,
                pattern,
                output,
                template_path,
                mods_dir,
            };

            benchmark::run(global_config, benchmark_config).await?;
        }
    }

    Ok(())
}
