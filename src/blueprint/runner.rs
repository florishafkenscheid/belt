//! Running and collecting logs of benchmarks on save file(s)

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::{fs, sync::atomic::Ordering};

use crate::core::{
    config::BlueprintConfig,
    error::{BenchmarkError, BenchmarkErrorKind},
    factorio::FactorioSaveRunSpec,
    settings::{ModSettings, ModSettingsScopeName, ModSettingsValue},
    utils, FactorioExecutor, Result,
};

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
            if !running.load(Ordering::SeqCst) {
                tracing::info!("Shutdown requested. Aborting remaining blueprints.");
                break;
            }

            // add prefix to bp file
            let orig_name = bp_file.file_name().and_then(|n| n.to_str()).ok_or(
                BenchmarkErrorKind::InvalidBlueprintFileName {
                    path: bp_file.to_path_buf(),
                },
            )?;

            let orig_stem = bp_file.file_stem().and_then(|s| s.to_str()).ok_or(
                BenchmarkErrorKind::InvalidBlueprintFileName {
                    path: bp_file.to_path_buf(),
                },
            )?;

            // Apply optional prefix to both name and stem
            let filestem = if let Some(prefix) = &self.config.prefix {
                // Compute new filename (prefix + original filename)
                let new_filename = format!("{prefix}{orig_name}");
                // Compute new stem (prefix + original stem)
                let new_filestem = format!("{prefix}{orig_stem}");

                // Rename the file on disk if not already renamed
                let new_path = bp_file.with_file_name(&new_filename);
                std::fs::rename(bp_file, &new_path)?;

                new_filestem
            } else {
                orig_stem.to_string()
            };

            // inject mod settings
            if let Some(ref mods_dir) = self.config.mods_dir.clone().or(utils::find_mod_directory())
            {
                tracing::debug!("Using mods-dir: {}", mods_dir.display());
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
                    "belt-sanitizer-blueprint-string",
                    Some(ModSettingsValue::String(blueprint_string)),
                );

                // Blueprint save name
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-blueprint-save-name",
                    Some(ModSettingsValue::String(filestem.clone())),
                );

                // Blueprint count
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-blueprint-count",
                    Some(ModSettingsValue::Int(self.config.count as i64)),
                );

                // Blueprint bot count
                if let Some(bot_count) = self.config.bot_count {
                    ms.set(
                        ModSettingsScopeName::Startup,
                        "belt-sanitizer-blueprint-bot-count",
                        Some(ModSettingsValue::Int(bot_count as i64)),
                    );
                }

                ms.save_to_file(dat_file)?;
            } else {
                return Err(
                    BenchmarkError::from(BenchmarkErrorKind::NoModsDirectoryFound)
                        .with_hint(Some("Please supply a --mods-dir explicitely.")),
                );
            }

            self.factorio
                .run_for_save(
                    FactorioSaveRunSpec {
                        base_save_file: &self.config.base_save_path,
                        new_save_name: filestem.clone(),
                        mods_dir: self.config.mods_dir.as_deref(),
                        headless: self.config.headless,
                    },
                    running,
                )
                .await?;

            // check existance
            if let Some(save_file) = utils::check_save_file(format!("_autosave-{}", &filestem)) {
                tracing::debug!("Found generated save file at: {}", save_file.display());

                if let Some(output_dir) = &self.config.output {
                    std::fs::rename(&save_file, output_dir.join(format!("{}.zip", &filestem)))?;
                    tracing::info!(
                        "Moved generated save from: {}, to: {}",
                        save_file.display(),
                        output_dir.display()
                    );
                }
            } else {
                tracing::error!("No generated save file found.");
            }
        }

        Ok(())
    }
}
