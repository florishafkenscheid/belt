//! Command-line flags, named as in B.E.L.T. 4.x. Every flag is optional here: [`Cli::flags`]
//! hands only the given ones to the settings layering, where they beat environment, config file
//! and defaults.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde_json::{Map, Value, json};

use crate::config::{RunOrder, split_list};

#[derive(Debug, Parser)]
#[command(
    name = "belt",
    about = "Factorio benchmarking tool",
    disable_version_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<CommandKind>,
    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Path to Factorio executable"
    )]
    pub factorio_path: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Enable verbose logging"
    )]
    pub verbose: bool,
    #[arg(
        long,
        global = true,
        env = "BELT_CONFIG",
        help_heading = "Global Options",
        help = "Path to config file (default: ~/.config/belt/config.toml)"
    )]
    pub config: Option<PathBuf>,
    #[arg(
        long,
        help_heading = "Global Options",
        help = "Initialize config directory with example config"
    )]
    pub init_config: bool,
    #[arg(
        long,
        help_heading = "Global Options",
        help = "Prints the BELT binary's version"
    )]
    pub version: bool,
    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "B.E.L.T.'s own data directory (mod cache, run directories)"
    )]
    pub workspace_dir: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help_heading = "Global Options",
        help = "Use this belt-sanitizer mod folder instead of the mod portal release"
    )]
    pub sanitizer_mod: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum CommandKind {
    /// Measure how fast Factorio simulates saves.
    #[command(next_help_heading = "Benchmark Options")]
    Benchmark(BenchmarkArgs),
    /// Build blueprints into a base save, many copies each, and save the results.
    #[command(next_help_heading = "Blueprint Options")]
    Blueprint(BlueprintArgs),
    /// Run saves and report what would skew a benchmark.
    #[command(next_help_heading = "Sanitize Options")]
    Sanitize(SanitizeArgs),
}

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
    /// Directory containing save files to benchmark
    #[arg(value_name = "SAVES_DIR")]
    pub saves_dir: Option<PathBuf>,
    #[arg(long, help = "Number of ticks to run each benchmark")]
    pub ticks: Option<u32>,
    #[arg(long, help = "Number of benchmark runs per save file")]
    pub runs: Option<u32>,
    #[arg(long, help = "Pattern to filter save files")]
    pub pattern: Option<String>,
    #[arg(long, help = "Output directory")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "Path to handlebars report template")]
    pub template_path: Option<PathBuf>,
    #[arg(long, help = "Directory containing mods to use")]
    pub mods_dir: Option<PathBuf>,
    #[arg(long, help = "Factorio's user data directory (mods, player-data.json)")]
    pub data_dir: Option<PathBuf>,
    #[arg(
        long,
        help = "Execution order: sequential (A,B,A,B), random (A,B,B,A), or grouped (A,A,B,B)"
    )]
    pub run_order: Option<RunOrder>,
    #[arg(
        long,
        help = "Export per-tick CSV data for specified Factorio benchmark metrics (e.g., 'wholeUpdate,gameUpdate'). Use 'all' to export all metrics."
    )]
    pub verbose_metrics: Option<String>,
    #[arg(long, help = "Prefix to strip from save file names in output")]
    pub strip_prefix: Option<String>,
    #[arg(long, help = "Record CPU frequency data during benchmark runs")]
    pub record_cpu: bool,
    #[arg(
        long,
        help = "Append the results of this benchmark to existing belt data as specified by --output"
    )]
    pub append: bool,
    #[arg(long, help = "Write verbose metric CSVs zstd-compressed (.csv.zst)")]
    pub compress: bool,
}

#[derive(Debug, Args)]
pub struct BlueprintArgs {
    /// Directory containing blueprint files
    pub blueprints_dir: PathBuf,
    /// Path to the base save file for blueprint testing
    pub base_save_path: PathBuf,
    #[arg(long, help = "Number of copies of each blueprint")]
    pub count: Option<u32>,
    #[arg(long, help = "Number of ticks to simulate before saving")]
    pub buffer_ticks: Option<u32>,
    #[arg(long, help = "Module that replaces marker modules in mining drills")]
    pub mining_module_replacement: Option<String>,
    #[arg(long, help = "Quality of the replacement module")]
    pub mining_module_replacement_quality: Option<String>,
    #[arg(long, help = "Directory containing mods to use")]
    pub mods_dir: Option<PathBuf>,
    #[arg(long, help = "Factorio's user data directory (mods, player-data.json)")]
    pub data_dir: Option<PathBuf>,
    #[arg(long, help = "Prefix for output file names")]
    pub prefix: Option<String>,
    #[arg(long, help = "Pattern to filter blueprint files")]
    pub pattern: Option<String>,
    #[arg(long, help = "Output directory")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "Number of logistic robots added to each copy's network")]
    pub bot_count: Option<u32>,
}

#[derive(Debug, Args)]
pub struct SanitizeArgs {
    /// Directory containing save files to sanitize
    #[arg(value_name = "SAVES_DIR")]
    pub saves_dir: Option<PathBuf>,
    #[arg(long, help = "Pattern to filter save files")]
    pub pattern: Option<String>,
    #[arg(long, help = "Number of ticks to run sanitization")]
    pub ticks: Option<u32>,
    #[arg(long, help = "Directory containing mods to use")]
    pub mods_dir: Option<PathBuf>,
    #[arg(long, help = "Factorio's user data directory (mods, player-data.json)")]
    pub data_dir: Option<PathBuf>,
    #[arg(long, help = "Items to track (comma-separated)")]
    pub items: Option<String>,
    #[arg(long, help = "Fluids to track (comma-separated)")]
    pub fluids: Option<String>,
}

/// Collects the given flags into `{section: {key: value}}`.
#[derive(Default)]
struct Layer(Map<String, Value>);

impl Layer {
    fn set(&mut self, section: &str, key: &str, value: Option<Value>) {
        let Some(value) = value else { return };
        let section = self
            .0
            .entry(section)
            .or_insert_with(|| Value::Object(Map::new()));
        section
            .as_object_mut()
            .expect("sections are objects")
            .insert(key.into(), value);
    }
}

fn given<T: serde::Serialize>(value: &Option<T>) -> Option<Value> {
    value.as_ref().map(|value| json!(value))
}

/// A switch that only ever turns its setting on, as 4.x switches do.
fn switch(given: bool) -> Option<Value> {
    if given { Some(json!(true)) } else { None }
}

impl Cli {
    pub fn flags(&self) -> Value {
        let mut layer = Layer::default();
        layer.set("global", "factorio_path", given(&self.factorio_path));
        layer.set("global", "verbose", switch(self.verbose));
        layer.set("global", "workspace_dir", given(&self.workspace_dir));
        layer.set("global", "sanitizer_mod", given(&self.sanitizer_mod));
        match &self.command {
            Some(CommandKind::Benchmark(args)) => {
                let section = "benchmark";
                layer.set(section, "saves_dir", given(&args.saves_dir));
                layer.set(section, "ticks", given(&args.ticks));
                layer.set(section, "runs", given(&args.runs));
                layer.set(section, "pattern", given(&args.pattern));
                layer.set(section, "output", given(&args.output));
                layer.set(section, "template_path", given(&args.template_path));
                layer.set(section, "mods_dir", given(&args.mods_dir));
                layer.set(section, "data_dir", given(&args.data_dir));
                layer.set(section, "run_order", given(&args.run_order));
                layer.set(
                    section,
                    "verbose_metrics",
                    args.verbose_metrics
                        .as_deref()
                        .map(|list| json!(split_list(list))),
                );
                layer.set(section, "strip_prefix", given(&args.strip_prefix));
                layer.set(section, "record_cpu", switch(args.record_cpu));
                layer.set(section, "append", switch(args.append));
                layer.set(section, "compress", switch(args.compress));
            }
            Some(CommandKind::Blueprint(args)) => {
                let section = "blueprint";
                layer.set(section, "blueprints_dir", Some(json!(args.blueprints_dir)));
                layer.set(section, "base_save_path", Some(json!(args.base_save_path)));
                layer.set(section, "count", given(&args.count));
                layer.set(section, "buffer_ticks", given(&args.buffer_ticks));
                layer.set(
                    section,
                    "mining_module_replacement",
                    given(&args.mining_module_replacement),
                );
                layer.set(
                    section,
                    "mining_module_replacement_quality",
                    given(&args.mining_module_replacement_quality),
                );
                layer.set(section, "mods_dir", given(&args.mods_dir));
                layer.set(section, "data_dir", given(&args.data_dir));
                layer.set(section, "prefix", given(&args.prefix));
                layer.set(section, "pattern", given(&args.pattern));
                layer.set(section, "output", given(&args.output));
                layer.set(section, "bot_count", given(&args.bot_count));
            }
            Some(CommandKind::Sanitize(args)) => {
                let section = "sanitize";
                layer.set(section, "saves_dir", given(&args.saves_dir));
                layer.set(section, "pattern", given(&args.pattern));
                layer.set(section, "ticks", given(&args.ticks));
                layer.set(section, "mods_dir", given(&args.mods_dir));
                layer.set(section, "data_dir", given(&args.data_dir));
                layer.set(section, "items", given(&args.items));
                layer.set(section, "fluids", given(&args.fluids));
            }
            None => {}
        }
        Value::Object(layer.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_4x_flag_names_reach_their_4x_config_keys() {
        let cli = Cli::parse_from([
            "belt",
            "--factorio-path",
            "/f",
            "benchmark",
            "/saves",
            "--run-order",
            "grouped",
            "--mods-dir",
            "/mods",
            "--record-cpu",
            "--verbose-metrics",
            "wholeUpdate,entityUpdate",
        ]);

        assert_eq!(
            cli.flags(),
            json!({
                "global": { "factorio_path": "/f" },
                "benchmark": {
                    "saves_dir": "/saves", "run_order": "grouped", "mods_dir": "/mods", "record_cpu": true,
                    "verbose_metrics": ["wholeUpdate", "entityUpdate"],
                },
            })
        );
    }

    #[test]
    fn headless_is_no_longer_a_flag() {
        assert!(Cli::try_parse_from(["belt", "--headless", "benchmark", "/saves"]).is_err());
    }
}
