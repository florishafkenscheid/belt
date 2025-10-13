//! Running and collecting logs of benchmarks on save file(s)

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::core::FactorioExecutor;
use crate::core::Result;
use crate::core::config::BlueprintConfig;
use crate::core::error::BenchmarkError;
use crate::core::error::BenchmarkErrorKind;
use crate::core::factorio::FactorioRunSpec;
use crate::core::settings::ModSettings;
use crate::core::settings::ModSettingsScopeName;
use crate::core::settings::ModSettingsValue;
use crate::core::utils;

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
        for bp_file in &blueprint_files {
            // add prefix to bp file
            let filename = if let Some(prefix) = &self.config.prefix {
                let new_name = bp_file.file_name().unwrap().to_str().unwrap();
                std::fs::rename(
                    bp_file,
                    bp_file.with_file_name(format!("{prefix}{new_name}")),
                )?;

                format!("{prefix}{new_name}")
            } else {
                bp_file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(BenchmarkErrorKind::InvalidBlueprintFileName {
                        path: bp_file.to_path_buf(),
                    })?
                    .to_string()
            };

            // inject mod settings
            if let Some(ref mods_dir) = self.config.mods_dir.clone().or(utils::find_mod_directory())
            {
                let dat_file = &mods_dir.join("mod-settings.dat");
                let mut ms = ModSettings::load_from_file(dat_file)?;
                // Target tick
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-target-tick",
                    Some(ModSettingsValue::Int(self.config.buffer_ticks as i64)),
                );

                // Blueprint mode
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-blueprint-mode",
                    Some(ModSettingsValue::Bool(true)), // Always set to true
                );

                // Blueprint string
                let blueprint_string = fs::read_to_string(bp_file)?;
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sainitizer-blueprint-string",
                    Some(ModSettingsValue::String(blueprint_string)),
                );

                // Blueprint count
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-blueprint-count",
                    Some(ModSettingsValue::Int(self.config.count as i64)),
                );
            } else {
                return Err(
                    BenchmarkError::from(BenchmarkErrorKind::NoModsDirectoryFound)
                        .with_hint(Some("Please supply a --mods-dir explicitely.")),
                );
            }

            let _output = self
                .factorio
                .run_for_ticks(
                    FactorioRunSpec {
                        save_file: self.config.base_save_path.as_path(),
                        ticks: self.config.buffer_ticks,
                        mods_dir: self.config.mods_dir.as_deref(),
                        verbose_all_metrics: false,
                        headless: self.config.headless,
                    },
                    running,
                )
                .await?;

            // check existance
            if let Some(save_file) = utils::check_save_file(&filename) {
                tracing::debug!("Found generated save file at: {}", save_file.display());

                if let Some(output_dir) = &self.config.output {
                    std::fs::rename(&save_file, output_dir.join(&filename))?;
                    tracing::info!(
                        "Moved generated save from: {}, to: {}",
                        save_file.display(),
                        output_dir.join(filename).display()
                    );
                }
            }
        }

        Ok(())
    }
}
