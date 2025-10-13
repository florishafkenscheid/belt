//! Running and collecting logs of benchmarks on save file(s)

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::core::FactorioExecutor;
use crate::core::Result;
use crate::core::config::BlueprintConfig;
use crate::core::factorio::FactorioRunSpec;

pub struct BlueprintRunner {
    config: BlueprintConfig,
    factorio: FactorioExecutor,
}

/// Runs the benchmarks, keeps a progress bar updated and returns results.
impl BlueprintRunner {
    pub fn new(config: BlueprintConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    /// Run benchmarks for all blueprint files
    pub async fn run_all(
        &self,
        blueprint_files: Vec<PathBuf>,
        running: &Arc<AtomicBool>,
    ) -> Result<()> {
        for _bp_file in &blueprint_files {
            // inject mod settings
            let _prefix = &self.config.prefix;

            let _output = self
                .factorio
                .run_for_ticks(
                    FactorioRunSpec {
                        save_file: self.config.base_save_path.as_path(),
                        ticks: 100, // Arbitrary, (should) happen in a few ticks
                        mods_dir: self.config.mods_dir.as_deref(),
                        verbose_all_metrics: false,
                        headless: self.config.headless,
                    },
                    running,
                )
                .await?;

            // check existance
            let _data_dir = &self.config.data_dir;
        }

        Ok(())
    }
}
