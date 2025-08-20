//! Finding and validating Factorio save files

use std::path::{Path, PathBuf};

use crate::core::error::{BenchmarkErrorKind, Result};

/// Find save files in a given path
pub fn find_save_files(saves_dir: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
    if !saves_dir.exists() {
        return Err(BenchmarkErrorKind::SaveDirectoryNotFound {
            path: saves_dir.to_path_buf(),
        }
        .into());
    }

    // If the given path is a file, check the extension and return
    if saves_dir.is_file() {
        if saves_dir.extension().is_some_and(|ext| ext == "zip") {
            return Ok(vec![saves_dir.to_path_buf()]);
        } else {
            return Err(BenchmarkErrorKind::InvalidSaveFile {
                path: saves_dir.to_path_buf(),
                reason: "Save file is not a .zip".to_string(),
            }
            .into());
        }
    }

    // Set up the whole pattern
    let pattern = pattern.unwrap_or("*");
    let search_pattern = saves_dir.join(format!("{pattern}.zip"));

    // Search using the pattern
    let saves: Vec<PathBuf> = glob::glob(search_pattern.to_string_lossy().as_ref())?
        .filter_map(std::result::Result::ok)
        .collect();

    // If empty, return
    if saves.is_empty() {
        return Err(BenchmarkErrorKind::NoSaveFilesFound {
            pattern: pattern.to_string(),
            directory: saves_dir.to_path_buf(),
        }
        .into());
    }

    tracing::info!("Found {} save files", saves.len());
    for save in &saves {
        tracing::debug!("  - {}", save.file_name().unwrap().to_string_lossy());
    }

    Ok(saves)
}

/// Validate found save files
pub fn validate_save_files(save_files: &[PathBuf]) -> Result<()> {
    for save_file in save_files {
        // Check if file exists
        if !save_file.exists() {
            return Err(BenchmarkErrorKind::InvalidSaveFile {
                path: save_file.clone(),
                reason: "File does not exist".to_string(),
            }
            .into());
        }

        // Check extension
        if save_file.extension().is_none_or(|ext| ext != "zip") {
            tracing::warn!(
                "Save file {} does not have .zip extension",
                save_file.display()
            );
        }
    }

    Ok(())
}
