use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::benchmark::BenchmarkConfig;
use crate::core::{BenchmarkError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub execution_time_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub effective_ups: f64,
    pub base_diff: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub save_name: String,
    pub ticks: u32,
    pub runs: Vec<BenchmarkRun>,
    pub factorio_version: String,
    pub platform: String,
}

pub fn parse_benchmark_log(
    log: &str,
    save_file: &Path,
    benchmark_config: &BenchmarkConfig,
) -> Result<BenchmarkResult> {
    let save_name = save_file.file_stem().unwrap().to_string_lossy().to_string();

    let version = log
        .lines()
        .find(|line| line.contains("Factorio") && line.contains("(build"))
        .and_then(|line| line.split_whitespace().nth(4))
        .unwrap_or("unknown")
        .to_string();

    let lines: Vec<&str> = log.lines().collect();

    let mut runs = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if let Some(line) = lines.get(i) {
            if line.contains("Performed") && line.contains("updates in") && line.contains("ms") {
                // e.g.: Performed 6000 updates in 2233.749 ms
                let parts: Vec<&str> = line.split_whitespace().collect();

                let execution_time_ms = parts
                    .get(4)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                if let Some(perf_line) = lines.get(i + 1) {
                    if perf_line.contains("avg:")
                        && perf_line.contains("min:")
                        && perf_line.contains("max:")
                    {
                        let parts: Vec<&str> = perf_line.split_whitespace().collect();

                        let avg_ms = parts
                            .iter()
                            .position(|&x| x == "avg:")
                            .and_then(|pos| parts.get(pos + 1))
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        let min_ms = parts
                            .iter()
                            .position(|&x| x == "min:")
                            .and_then(|pos| parts.get(pos + 1))
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        let max_ms = parts
                            .iter()
                            .position(|&x| x == "max:")
                            .and_then(|pos| parts.get(pos + 1))
                            .and_then(|s| s.parse::<f64>().ok())
                            .unwrap_or(0.0);

                        let effective_ups = if execution_time_ms > 0.0 {
                            1000.0 * benchmark_config.ticks as f64 / execution_time_ms
                        } else {
                            0.0
                        };

                        runs.push(BenchmarkRun {
                            execution_time_ms,
                            avg_ms,
                            min_ms,
                            max_ms,
                            effective_ups,
                            base_diff: 0.0, // Will be calculated later
                        });
                    }
                }
            }
        }
        i += 1;
    }

    if runs.is_empty() {
        return Err(BenchmarkError::NoBenchmarkResults);
    }

    Ok(BenchmarkResult {
        save_name,
        ticks: benchmark_config.ticks,
        runs,
        factorio_version: version,
        platform: crate::core::platform::get_os_info(),
    })
}

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
