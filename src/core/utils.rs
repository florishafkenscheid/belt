//! Utility functions for BELT.

use std::{path::Path, time::Duration};

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

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::benchmark::parser::BenchmarkResult;

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
