//! Utility functions for BELT.

use std::collections::HashMap;
use std::path::PathBuf;
use std::{path::Path, time::Duration};

// Structs & Impls
#[derive(Debug, Clone, Default)]
pub enum RunOrder {
    Sequential,
    Random,
    #[default]
    Grouped,
}

// Get a RunOrder from a string
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

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::benchmark::parser::BenchmarkResult;
use crate::benchmark::runner::VerboseData;
use crate::core::error::BenchmarkErrorKind;
use crate::Result;

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
    for path in get_default_user_data_dirs() {
        if path.join("script-output/belt").exists() {
            return Some(path.join("script-output/belt"));
        }
    }
    None
}

/// Tries to find [user data directory](https://wiki.factorio.com/Application_directory#User_data_directory)
fn get_default_user_data_dirs() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    
    let Some(home) = dirs::home_dir() else {
        return paths;
    };

    if cfg!(target_os = "windows") {
        paths.push(home.join("Factorio"));
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
pub fn calculate_base_differences(results: &mut [BenchmarkResult]) {
    // Calculate average effective_ups for each save
    let avg_ups_per_save: Vec<f64> = results
        .iter()
        .map(|result| {
            let total_ups: f64 = result.runs.iter().map(|run| run.effective_ups).sum();
            total_ups / result.runs.len() as f64
        })
        .collect();

    // Find the minimum average effective_ups across all saves
    let min_avg_ups = avg_ups_per_save
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .copied()
        .unwrap_or(0.0);

    // Calculate base_diff as percentage improvement for each run relative to the worst-performing save's average
    for (result_idx, result) in results.iter_mut().enumerate() {
        let save_avg_ups = avg_ups_per_save[result_idx];
        let percentage_improvement = if min_avg_ups > 0.0 {
            ((save_avg_ups - min_avg_ups) / min_avg_ups) * 100.0
        } else {
            0.0
        };

        for run in result.runs.iter_mut() {
            run.base_diff = percentage_improvement;
        }
    }
}

/// Calculate simple moving average
pub fn calculate_sma(data: &[f64], window_size: u32) -> Vec<f64> {
    if window_size == 0 || data.is_empty() {
        return data.to_vec(); // No smoothing or no data
    }

    let window_size = window_size as usize;
    let mut smoothed_data = Vec::with_capacity(data.len());
    let mut current_sum: f64 = 0.0;
    let mut window_count: usize = 0;

    for i in 0..data.len() {
        current_sum += data[i];
        window_count += 1;

        if i >= window_size {
            // Remove the oldest element that's falling out of the window
            current_sum -= data[i - window_size];
            window_count -= 1;
        }

        let avg = if window_count > 0 {
            current_sum / window_count as f64
        } else {
            0.0
        };
        smoothed_data.push(avg);
    }
    smoothed_data
}

struct BoxplotData {
    boxplot_values: Vec<Vec<f64>>,
    outlier_values: Vec<Vec<f64>>,
    category_names: Vec<String>,
    min_value: f64,
    max_value: f64,
}

/// Manually calculate the boxplot data given the benchmark results
pub fn calculate_boxplot_data(results: &[BenchmarkResult]) -> BoxplotData {
    // Collect save names
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    let mut grouped_boxplot_data: Vec<Vec<f64>> = Vec::new();
    let mut outliers: Vec<(usize, f64)> = Vec::new();
    let mut all_individual_ups: Vec<f64> = Vec::new();

    // Iterate over every result and push UPS values
    for result in results {
        let mut values: Vec<f64> = result.runs.iter().map(|run| run.effective_ups).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        all_individual_ups.extend(&values);
        grouped_boxplot_data.push(values);
    }

    // Calculate boxplot statistics manually
    let mut boxplot_data: Vec<Vec<f64>> = Vec::new();

    for (category_idx, values) in grouped_boxplot_data.iter().enumerate() {
        if values.is_empty() {
            continue;
        };

        let len = values.len();
        let q1_idx = len / 4;
        let q2_idx = len / 2;
        let q3_idx = (3 * len) / 4;

        let q1 = values[q1_idx];
        let q2 = values[q2_idx]; // median
        let q3 = values[q3_idx];
        let iqr = q3 - q1;

        let lower_fence = q1 - 1.5 * iqr;
        let upper_fence = q3 + 1.5 * iqr;

        // Find whiskers (actual min/max within fences)
        let lower_whisker = values
            .iter()
            .find(|&&v| v >= lower_fence)
            .unwrap_or(&values[0]);
        let upper_whisker = values
            .iter()
            .rev()
            .find(|&&v| v <= upper_fence)
            .unwrap_or(&values[len - 1]);

        // Collect outliers
        for &value in values {
            if value < lower_fence || value > upper_fence {
                outliers.push((category_idx, value));
            }
        }

        // Boxplot data format: [min, Q1, median, Q3, max]
        boxplot_data.push(vec![*lower_whisker, q1, q2, q3, *upper_whisker]);
    }

    let min_ups = all_individual_ups
        .iter()
        .cloned()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    let max_ups = all_individual_ups
        .iter()
        .cloned()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    // Convert outliers to scatter data
    let scatter_data: Vec<Vec<f64>> = outliers
        .into_iter()
        .map(|(category, value)| vec![category as f64, value])
        .collect();

    BoxplotData {
        boxplot_values: boxplot_data,
        outlier_values: scatter_data,
        category_names: save_names,
        min_value: min_ups,
        max_value: max_ups,
    }
}

/// Compute global min/max for each metric across all saves and runs
pub fn compute_global_metric_bounds(
    all_verbose_data: &[VerboseData],
    metrics_to_chart: &[String],
    smooth_window: u32,
) -> HashMap<String, (f64, f64)> {
    let mut bounds: HashMap<String, (f64, f64)> = HashMap::new();

    if all_verbose_data.is_empty() {
        return bounds;
    }

    let mut reader = csv::Reader::from_reader(all_verbose_data[0].csv_data.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .unwrap()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let header_map: HashMap<String, usize> = headers
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    for metric_name in metrics_to_chart {
        let mut all_smoothed_ns: Vec<f64> = Vec::new();

        if let Some(&column_index) = header_map.get(metric_name) {
            for run_data in all_verbose_data {
                let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
                let mut current_run_raw_values_ns: Vec<f64> = Vec::new();

                for record_result in inner_reader.records() {
                    let record = record_result.unwrap();
                    if let Some(value_ns_str) = record.get(column_index)
                        && let Ok(value_ns) = value_ns_str.parse::<f64>()
                    {
                        current_run_raw_values_ns.push(value_ns);
                    }
                }
                let smoothed_run_values_ns =
                    calculate_sma(&current_run_raw_values_ns, smooth_window);
                all_smoothed_ns.extend(smoothed_run_values_ns);
            }
        }

        if !all_smoothed_ns.is_empty() {
            let n = all_smoothed_ns.len() as f64;
            let mean = all_smoothed_ns.iter().sum::<f64>() / n;
            let stddev = (all_smoothed_ns
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();

            let min_ns = (mean - 2.0 * stddev).max(0.0);
            let max_ns = mean + 2.0 * stddev;

            let min_ms = min_ns / 1_000_000.0;
            let max_ms = max_ns / 1_000_000.0;

            let (min_ms, max_ms) = if min_ms == max_ms {
                let new_min = (min_ms * 0.9).max(0.0);
                let new_max = (max_ms * 1.1).max(0.1);
                (new_min, new_max)
            } else {
                (min_ms, max_ms)
            };

            bounds.insert(metric_name.clone(), (min_ms, max_ms));
        }
    }

    bounds
}

/// Get operating system info
pub fn get_os_info() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}