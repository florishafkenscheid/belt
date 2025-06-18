use std::path::{Path, PathBuf};

use crate::core::error::{BenchmarkError, Result};

pub fn find_save_files(saves_dir: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
    if !saves_dir.exists() {
        return Err(BenchmarkError::SaveDirectoryNotFound { path: saves_dir.to_path_buf() });
    }

    if saves_dir.is_file() {
        if saves_dir.extension().map_or(false, |ext| ext == "zip") {
            return Ok(vec![saves_dir.to_path_buf()]);
        } else {
            return Err(BenchmarkError::InvalidSaveFile { path: saves_dir.to_path_buf(), reason: "Save file is not a .zip".to_string() });
        }
    }

    let pattern = pattern.unwrap_or("");
    let search_pattern = saves_dir.join(format!("{}*.zip", pattern));

    let saves: Vec<PathBuf> = glob::glob(search_pattern.to_string_lossy().as_ref())?
        .filter_map(std::result::Result::ok)
        .collect();

    if saves.is_empty() {
        return Err(BenchmarkError::NoSaveFilesFound { pattern: pattern.to_string(), directory: saves_dir.to_path_buf() });
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
            return Err(BenchmarkError::InvalidSaveFile { path: save_file.clone(), reason: "File does not exist".to_string() });
        }
        
        if !save_file.extension().map_or(false, |ext| ext == "zip") {
            tracing::warn!("Save file {} does not have .zip extension", save_file.display());
        }
    }
    
    Ok(())
}
