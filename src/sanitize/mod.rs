pub mod parser;
pub mod runner;

use std::sync::{Arc, atomic::AtomicBool};

use crate::{
    Result,
    core::{
        FactorioExecutor,
        config::{GlobalConfig, SanitizeConfig},
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

    let mut adjusted_config = sanitize_config.clone();
    adjusted_config.ticks = adjusted_ticks;

    // Report
    let runner = runner::SanitizeRunner::new(adjusted_config, factorio);
    runner.run_all(save_files, running).await?;

    Ok(())
}
