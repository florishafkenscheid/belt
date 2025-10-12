//! Running and collecting logs of benchmarks on save file(s)

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::BlueprintBenchmarkConfig;
use crate::benchmark::parser::BenchmarkResult;
use crate::benchmark::runner::{BenchmarkRunner, VerboseData};
use crate::core::FactorioExecutor;
use crate::core::Result;
use crate::core::config::BenchmarkConfig;
use crate::core::factorio::FactorioBlueprintRunSpec;

pub struct BlueprintBenchmarkRunner {
    config: BlueprintBenchmarkConfig,
    factorio: FactorioExecutor,
}

/// Runs the benchmarks, keeps a progress bar updated and returns results.
impl BlueprintBenchmarkRunner {
    pub fn new(config: BlueprintBenchmarkConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    /// Run benchmarks for all blueprint files
    pub async fn run_all(
        &self,
        blueprint_files: Vec<PathBuf>,
        running: &Arc<AtomicBool>,
    ) -> Result<(Vec<BenchmarkResult>, Vec<VerboseData>)> {
        // convert all the bps, then run with the regular runner
        let mut converted_save_paths: Vec<PathBuf> = Vec::new();
        for bp_file in &blueprint_files {
            let new_save_path = self
                .factorio
                .run_for_blueprint(
                    FactorioBlueprintRunSpec {
                        save_file: self.config.base_save_path.as_path(),
                        blueprint_file: bp_file.as_path(),
                        blueprint_stable_ticks: self.config.blueprint_stable_ticks,
                        mods_dir: self.config.mods_dir.as_path(),
                        data_dir: self.config.data_dir.as_path(),
                        headless: self.config.headless,
                    },
                    running,
                )
                .await?;
            converted_save_paths.push(new_save_path);
        }
        return BenchmarkRunner::new(
            BenchmarkConfig {
                saves_dir: self.config.blueprints_dir.clone(), // doesn't matter, not really used...
                ticks: self.config.ticks,
                runs: self.config.runs,
                pattern: self.config.pattern.clone(),
                output: self.config.output.clone(),
                template_path: self.config.template_path.clone(),
                mods_dir: Some(self.config.mods_dir.clone()),
                run_order: self.config.run_order.clone(),
                verbose_metrics: self.config.verbose_metrics.clone(),
                strip_prefix: self.config.strip_prefix.clone(),
                headless: self.config.headless,
            },
            FactorioExecutor::new(self.factorio.executable_path().to_path_buf()),
        )
        .run_all(converted_save_paths, running)
        .await;
    }
}
