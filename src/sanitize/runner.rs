//! Running and collecting logs of sanitization on save file(s)

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use indicatif::{ProgressBar, ProgressStyle};

use crate::{
    Result,
    core::{
        FactorioExecutor,
        config::SanitizeConfig,
        factorio::FactorioTickRunSpec,
        format_duration,
        settings::{ModSettings, ModSettingsScopeName, ModSettingsValue},
        utils,
    },
    sanitize::parser,
};

pub struct SanitizeRunner {
    config: SanitizeConfig,
    factorio: FactorioExecutor,
}

impl SanitizeRunner {
    pub fn new(config: SanitizeConfig, factorio: FactorioExecutor) -> Self {
        Self { config, factorio }
    }

    pub async fn run_all(&self, save_files: Vec<PathBuf>, running: &Arc<AtomicBool>) -> Result<()> {
        let total_jobs = save_files.len();
        let start_time = Instant::now();

        let progress = ProgressBar::new(total_jobs as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )?
            .progress_chars("=="),
        );
        progress.enable_steady_tick(Duration::from_millis(100));

        for (idx, save_file) in save_files.iter().enumerate() {
            if !running.load(Ordering::SeqCst) {
                tracing::info!("Shutdown requested. Aborting remaining sanitization.");
                break;
            }

            progress.set_position(idx as u64);

            let save_name = save_file
                .file_stem()
                .expect("save file stem")
                .to_string_lossy()
                .to_string();

            if idx > 0 {
                let elapsed = start_time.elapsed();
                let avg = elapsed / idx as u32;
                let remain = total_jobs - idx;
                let eta = avg * remain as u32;
                progress.set_message(format!("{} [ETA: {}]", save_name, format_duration(eta)));
            } else {
                progress.set_message(save_name.clone());
            }

            if self.config.mods_dir.is_none() {
                self.factorio.sync_mods_for_save(save_file).await?;
            }

            // Update belt-sanitizer mod settings
            if let Some(ref mods_dir) = self.config.mods_dir.clone().or(utils::find_mod_directory())
            {
                let dat_file = &mods_dir.join("mod-settings.dat");
                let mut ms = ModSettings::load_from_file(dat_file)?;

                // Disable blueprint-mode just to be sure
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-blueprint-mode",
                    Some(ModSettingsValue::Bool(false)),
                );

                // Prod check tick
                ms.set(
                    ModSettingsScopeName::Startup,
                    "belt-sanitizer-target-tick",
                    Some(ModSettingsValue::Int(self.config.ticks as i64)),
                );

                // Items
                if let Some(ref items) = self.config.items {
                    ms.set(
                        ModSettingsScopeName::Startup,
                        "belt-sanitizer-production-items",
                        Some(ModSettingsValue::String(items.clone())),
                    );
                }

                // Fluids
                if let Some(ref fluids) = self.config.fluids {
                    ms.set(
                        ModSettingsScopeName::Startup,
                        "belt-sanitizer-production-fluids",
                        Some(ModSettingsValue::String(fluids.clone())),
                    );
                }

                ms.save_to_file(dat_file)?;
            }

            let _output = self
                .factorio
                .run_for_ticks(FactorioTickRunSpec {
                    save_file,
                    ticks: self.config.ticks,
                    mods_dir: self.config.mods_dir.as_deref(),
                    verbose_all_metrics: false,
                    headless: self.config.headless,
                })
                .await?;

            parser::report(&self.config)?;
        }

        if !running.load(Ordering::SeqCst) {
            progress.finish_with_message("Sanitization interrupted");
        } else {
            progress.finish_with_message("Sanitization complete!");
        }

        Ok(())
    }
}
