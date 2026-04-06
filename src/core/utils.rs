//! Utility functions for BELT.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;
use crate::benchmark::parser::BenchmarkRun;
use crate::sanitize::parser::ProductionStatistic;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{path::Path, time::Duration};

// Structs & Impls
/// Execution order for benchmark runs
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunOrder {
    /// Run benchmarks in sequential order (A, B, A, B)
    Sequential,
    /// Run benchmarks in random order
    Random,
    /// Run benchmarks in grouped order (A, A, B, B) - default
    #[default]
    Grouped,
}

/// Get a RunOrder from a string
impl std::str::FromStr for RunOrder {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sequential" => Ok(RunOrder::Sequential),
            "random" => Ok(RunOrder::Random),
            "grouped" => Ok(RunOrder::Grouped),
            _ => Err(BenchmarkErrorKind::InvalidRunOrder {
                input: s.to_string(),
            }
            .to_string()),
        }
    }
}

// Formatting related utilities
/// Helper function to turn a Duration into a nicely formatted string
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m{secs}s")
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        format!("{hours}h{mins}m")
    }
}

pub fn process_items(obj: &Value, stat_type: &str, items_vec: &mut Vec<ProductionStatistic>) {
    if let Some(items_obj) = obj.get("items").and_then(|x| x.as_object()) {
        for (item_name, quality_map) in items_obj {
            if let Some(qualities) = quality_map.as_object() {
                for (quality, count_val) in qualities {
                    let count = match count_val.as_f64() {
                        Some(c) => c as f32,
                        None => {
                            tracing::error!(
                                "Invalid count for {} {} {}: {:?}",
                                stat_type,
                                item_name,
                                quality,
                                count_val
                            );
                            0.0
                        }
                    };
                    items_vec.push(ProductionStatistic {
                        statistic_type: stat_type.to_string(),
                        name: item_name.clone(),
                        quality: Some(quality.clone()),
                        count,
                    });
                }
            }
        }
    }
}

pub fn process_fluids(obj: &Value, stat_type: &str, fluids_vec: &mut Vec<ProductionStatistic>) {
    if let Some(fluids_obj) = obj.get("fluids").and_then(|x| x.as_object()) {
        for (fluid_name, count_val) in fluids_obj {
            let count = match count_val.as_f64() {
                Some(c) => c as f32,
                None => {
                    tracing::error!(
                        "Invalid count for fluid {stat_type} {fluid_name}: {count_val:?}"
                    );
                    0.0
                }
            };
            fluids_vec.push(ProductionStatistic {
                statistic_type: stat_type.to_string(),
                name: fluid_name.clone(),
                quality: None,
                count,
            });
        }
    }
}

// File related utilities
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

pub fn find_blueprint_files(blueprint_dir: &Path, pattern: Option<&str>) -> Result<Vec<PathBuf>> {
    if !blueprint_dir.exists() {
        return Err(BenchmarkErrorKind::BlueprintDirectoryNotFound {
            path: blueprint_dir.to_path_buf(),
        }
        .into());
    }

    // If the given path is a file that is ok
    if blueprint_dir.is_file() {
        return Ok(vec![blueprint_dir.to_path_buf()]);
    }

    // Set up the whole pattern
    let pattern = pattern.unwrap_or("*");
    let search_pattern = blueprint_dir.join(pattern);

    // Search using the pattern
    let bps: Vec<PathBuf> = glob::glob(search_pattern.to_string_lossy().as_ref())?
        .filter_map(std::result::Result::ok)
        .collect();

    // If empty, return
    if bps.is_empty() {
        return Err(BenchmarkErrorKind::NoBlueprintFilesFound {
            pattern: pattern.to_string(),
            directory: blueprint_dir.to_path_buf(),
        }
        .into());
    }

    tracing::info!("Found {} blueprint files", bps.len());
    for bp in &bps {
        tracing::debug!("  - {}", bp.file_name().unwrap().to_string_lossy());
    }

    Ok(bps)
}

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::core::error::BenchmarkErrorKind;

/// Check if a file is an executable.
pub fn is_executable(path: &Path) -> bool {
    // On unix, check the 'execute' permission bit
    #[cfg(unix)]
    {
        fs::metadata(path).is_ok_and(|metadata| {
            metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0)
        })
    }

    #[cfg(windows)]
    {
        path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    }

    // Fallback for other operating systems.
    #[cfg(not(any(unix, windows)))]
    {
        metadata.is_file()
    }
}

/// Check if the belt-sanitizer mod is active
pub fn check_sanitizer() -> Option<PathBuf> {
    get_default_user_data_dirs()
        .iter()
        .map(|base| base.join(PathBuf::from("script-output/belt")))
        .find(|candidate| candidate.is_dir())
}

/// Check if the belt-sanitizer blueprint save file exists
pub fn check_save_file(name: String) -> Option<PathBuf> {
    get_default_user_data_dirs()
        .iter()
        .map(|base| base.join(format!("saves/{name}.zip")))
        .find(|path| path.exists())
}

/// Find mod directory
pub fn find_mod_directory() -> Option<PathBuf> {
    get_default_user_data_dirs()
        .iter()
        .map(|base| base.join("mods"))
        .find(|path| path.is_dir())
}

/// Tries to find [user data directory](https://wiki.factorio.com/Application_directory#User_data_directory)
fn get_default_user_data_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let Some(home) = dirs::home_dir() else {
        return paths;
    };

    if cfg!(target_os = "windows") {
        paths.push(home.join("AppData/Roaming/Factorio"));
    } else if cfg!(target_os = "linux") {
        paths.push(home.join(".factorio"));
        // Flatpak installations
        paths.push(home.join(".var/app/com.valvesoftware.Steam/.factorio"));
    } else if cfg!(target_os = "macos") {
        paths.push(home.join("Library/Application Support/factorio"));
    }

    paths
}

// Math related utilities
/// Calculate the base differences of a list of save's results.
pub fn calculate_base_differences(runs: &mut [BenchmarkRun]) {
    // save_name -> (sum_ups, count)
    let mut sums: BTreeMap<String, (f64, u32)> = BTreeMap::new();

    for r in runs.iter() {
        let entry = sums.entry(r.save_name.clone()).or_insert((0.0, 0));
        entry.0 += r.effective_ups;
        entry.1 += 1;
    }

    let min_avg_ups = sums
        .values()
        .map(|&(sum, n)| if n == 0 { 0.0 } else { sum / n as f64 })
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    for r in runs.iter_mut() {
        let (sum, n) = sums.get(&r.save_name).copied().unwrap_or((0.0, 0));
        let save_avg_ups = if n == 0 { 0.0 } else { sum / n as f64 };

        r.base_diff = if min_avg_ups > 0.0 {
            ((save_avg_ups - min_avg_ups) / min_avg_ups) * 100.0
        } else {
            0.0
        };
    }
}

pub fn round_to_precision_window(ticks: u32) -> u32 {
    const ONE_MINUTE: u32 = 3600;
    const TEN_MINUTES: u32 = 36000;
    const ONE_HOUR: u32 = 216000;
    const TEN_HOURS: u32 = 2160000;
    const FIFTY_HOURS: u32 = 10800000;
    const TWO_FIFTY_HOURS: u32 = 54000000;
    const FIVE_SECONDS: u32 = 300;

    // Find the appropriate window size and round up to nearest multiple
    let window = if ticks >= TWO_FIFTY_HOURS {
        TWO_FIFTY_HOURS
    } else if ticks >= FIFTY_HOURS {
        FIFTY_HOURS
    } else if ticks >= TEN_HOURS {
        TEN_HOURS
    } else if ticks >= ONE_HOUR {
        ONE_HOUR
    } else if ticks >= TEN_MINUTES {
        TEN_MINUTES
    } else if ticks >= ONE_MINUTE {
        ONE_MINUTE
    } else {
        FIVE_SECONDS
    };

    // Round up to nearest multiple of window
    ticks.div_ceil(window) * window
}

/// Get operating system info
pub fn get_os_info() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
