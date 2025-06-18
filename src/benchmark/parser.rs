use serde::{Serialize, Deserialize};
use std::path::Path;

use crate::{benchmark::BenchmarkConfig};
use crate::core::{BenchmarkError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub save_name: String,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub ticks: u32,
    pub total_execution_time_ms: u64,
    pub avg_effective_ups: f64,
    pub factorio_version: String,
    pub platform: String,
}

pub fn parse_benchmark_log(log: &str, save_file: &Path, benchmark_config: &BenchmarkConfig) -> Result<BenchmarkResult> {
        let save_name = save_file.file_stem().unwrap().to_string_lossy().to_string();
        
        let version = log.lines()
        .find(|line| line.contains("Factorio") && line.contains("(build"))
        .and_then(|line| {
            line.split_whitespace()
                .nth(4)
        })
        .unwrap_or("unknown")
        .to_string();

        let lines: Vec<&str> = log.lines().collect();

        let mut total_execution_time_ms = 0.0;
        let mut avg_ms_sum = 0.0;
        let mut min_ms = f64::MAX;
        let mut max_ms = f64::MIN;
        let mut effective_ups_sum = 0.0;
        let mut run_count = 0;

        let mut i = 0;

        while i < lines.len() {
            if let Some(line) = lines.get(i) {
                if line.contains("Performed") && line.contains("updates in") && line.contains("ms") {
                    run_count += 1;
                    
                    // e.g.: Performed 6000 updates in 2233.749 ms
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    
                    let execution_time_ms = parts.get(4)
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    total_execution_time_ms += execution_time_ms;

                    if let Some(perf_line) = lines.get(i + 1) {
                        if perf_line.contains("avg:") && perf_line.contains("min:") && perf_line.contains("max:") {
                            let parts: Vec<&str> = perf_line.split_whitespace().collect();

                            let avg_ms = parts.iter()
                                .position(|&x| x == "avg:")
                                .and_then(|pos| parts.get(pos+1))
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            avg_ms_sum += avg_ms;

                            let current_min = parts.iter()
                                .position(|&x| x == "min:")
                                .and_then(|pos| parts.get(pos+1))
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            min_ms = min_ms.min(current_min);

                            let current_max = parts.iter()
                                .position(|&x| x == "max:")
                                .and_then(|pos| parts.get(pos+1))
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            max_ms = max_ms.max(current_max);

                            let effective_ups = if execution_time_ms > 0.0 {
                                1000.0 * benchmark_config.ticks as f64 / execution_time_ms
                            } else {
                                0.0
                            };

                            effective_ups_sum += effective_ups;
                        }
                    }
                }
            }
            i += 1;
        }

        if run_count == 0 {
            return Err(BenchmarkError::NoBenchmarkResults);
        }

        Ok(BenchmarkResult {
            save_name,
            avg_ms: avg_ms_sum / run_count as f64,
            min_ms,
            max_ms,
            ticks: benchmark_config.ticks,
            total_execution_time_ms: total_execution_time_ms as u64,
            avg_effective_ups: effective_ups_sum / run_count as f64,
            factorio_version: version,
            platform: crate::core::platform::get_os_info(),
        })
    }
