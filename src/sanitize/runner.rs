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
    core::{FactorioExecutor, config::SanitizeConfig, factorio::FactorioRunSpec, format_duration},
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

            let _output = self
                .factorio
                .run_for_ticks(
                    FactorioRunSpec {
                        save_file,
                        ticks: self.config.ticks,
                        mods_dir: self.config.mods_dir.as_deref(),
                        verbose_all_metrics: false,
                        headless: self.config.headless,
                    },
                    running,
                )
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
