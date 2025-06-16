use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

pub fn find_save_files(saves_dir: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
    if !saves_dir.exists() {
        anyhow::bail!("Save directory does not exist: {}", saves_dir.display());
    }

    let pattern = pattern.unwrap_or("");
    let search_pattern = saves_dir.join(format!("{}*.zip", pattern));

    let saves: Vec<PathBuf> = glob::glob(search_pattern.to_string_lossy().as_ref())
        .context("Failed to read save directory")?
        .filter_map(Result::ok)
        .collect();

    if saves.is_empty() {
        anyhow::bail!("No save files found matching pattern '{}' in {}", pattern, saves_dir.display());
    }

    tracing::info!("Found {} save files", saves.len());
    for save in &saves {
        tracing::debug!("  - {}", save.file_name().unwrap().to_string_lossy());
    }

    Ok(saves)
}

pub fn validate_save_files(save_files: &[PathBuf]) -> Result<()> {
    for save_file in save_files {
        if !save_file.exists() {
            anyhow::bail!("Save file does not exist: {}", save_file.display());
        }
        
        if !save_file.extension().map_or(false, |ext| ext == "zip") {
            tracing::warn!("Save file {} does not have .zip extension", save_file.display());
        }
    }
    
    Ok(())
}
