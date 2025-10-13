pub mod parser;
pub mod runner;

use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    Result,
    core::{
        FactorioExecutor,
        config::{GlobalConfig, SanitizeConfig},
        settings::{ModSettings, ModSettingsScopeName, ModSettingsValue},
        utils,
    },
};

pub async fn run(
    global_config: GlobalConfig,
    sanitize_config: SanitizeConfig,
    running: &Arc<AtomicBool>,
) -> Result<()> {
    // Find the Factorio binary
    let factorio = FactorioExecutor::discover(global_config.factorio_path)?;
    tracing::info!(
        "Using Factorio at: {}",
        factorio.executable_path().display()
    );

    // Find the specified save files
    let save_files = utils::find_save_files(
        &sanitize_config.saves_dir,
        sanitize_config.pattern.as_deref(),
    )?;
    // Validate the found save files
    utils::validate_save_files(&save_files)?;

    // Round ticks to nearest precision window boundary
    let adjusted_ticks = utils::round_to_precision_window(sanitize_config.ticks);
    if adjusted_ticks != sanitize_config.ticks {
        tracing::info!(
            "Adjusted tick count from {} to {} to align with Factorio flow statistics windows",
            sanitize_config.ticks,
            adjusted_ticks
        );
    }

    // Update belt-sanitizer mod settings
    if let Some(ref mods_dir) = sanitize_config.mods_dir {
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
            Some(ModSettingsValue::Int(adjusted_ticks as i64)),
        );

        // Items
        if let Some(ref items) = sanitize_config.items {
            ms.set(
                ModSettingsScopeName::Startup,
                "belt-sanitizer-production-items",
                Some(ModSettingsValue::String(items.clone())),
            );
        }

        // Fluids
        if let Some(ref fluids) = sanitize_config.fluids {
            ms.set(
                ModSettingsScopeName::Startup,
                "belt-sanitizer-production-fluids",
                Some(ModSettingsValue::String(fluids.clone())),
            );
        }

        ms.save_to_file(dat_file)?;
    }

    let mut adjusted_config = sanitize_config.clone();
    adjusted_config.ticks = adjusted_ticks;

    // Report
    let runner = runner::SanitizeRunner::new(adjusted_config, factorio);
    runner.run_all(save_files, running).await?;

    Ok(())
}
