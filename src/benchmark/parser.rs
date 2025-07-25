//! Parsing and aggregation of Factorio benchmark logs

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::benchmark::BenchmarkConfig;
use crate::core::{BenchmarkError, Result};

/// The result of a benchmark of a single run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub execution_time_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub effective_ups: f64,
    pub base_diff: f64,
}

/// The result of a benchmark of a file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub save_name: String,
    pub ticks: u32,
    pub runs: Vec<BenchmarkRun>,
    pub factorio_version: String,
    pub platform: String,
}

/// Parsing of the given Factorio output
pub fn parse_benchmark_log(
    log: &str,
    save_file: &Path,
    benchmark_config: &BenchmarkConfig,
) -> Result<BenchmarkResult> {
    // Get save name from file
    let save_name = save_file.file_stem().unwrap().to_string_lossy().to_string();

    let save_name = match benchmark_config.strip_prefix.as_deref() {
        Some(prefix) => save_name
            .strip_prefix(prefix)
            .unwrap_or(&save_name)
            .to_string(),
        None => save_name,
    };

    // Get the Factorio version from the line containing "Factorio" and "(build"
    let version = log
        .lines()
        .find(|line| line.contains("Factorio") && line.contains("(build"))
        .and_then(|line| line.split_whitespace().nth(4))
        .unwrap_or("unknown")
        .to_string();

    // Collect all lines of the log
    let lines: Vec<&str> = log.lines().collect();

    let mut runs = Vec::new();
    let mut i = 0;

    // Iterate over every line, checking for keywords that indicate resulting data
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_base_differences_simple() {
        let mut results = vec![
            BenchmarkResult {
                save_name: "base_save".to_string(),
                runs: vec![BenchmarkRun {
                    effective_ups: 50.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
            BenchmarkResult {
                save_name: "fast_save".to_string(),
                runs: vec![BenchmarkRun {
                    effective_ups: 100.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
        ];

        calculate_base_differences(&mut results);

        assert_eq!(
            results[0].runs[0].base_diff, 0.0,
            "The worst-performing save should have 0% improvement"
        );
        assert_eq!(
            results[1].runs[0].base_diff, 100.0,
            "A save with double the UPS should show 100% improvement"
        );
    }

    #[test]
    fn test_parse_benchmark_log() {
        // Abridged output
        const FACTORIO_OUTPUT: &str = r#"0.000 2025-07-09 17:16:57; Factorio 2.0.55 (build 83138, linux64, full, space-age)
   Performed 1000 updates in 2138.223 ms
   avg: 2.138 ms, min: 1.367 ms, max: 11.710 ms
   checksum: 2846200395
   7.737 Goodbye"#;

        let save_path = Path::new("test_save.zip");

        let config = BenchmarkConfig {
            ticks: 1000,
            ..Default::default()
        };

        let result = parse_benchmark_log(FACTORIO_OUTPUT, save_path, &config).unwrap();

        // Check misc info
        assert_eq!(result.save_name, "test_save");
        assert_eq!(result.factorio_version, "2.0.55");
        assert_eq!(result.ticks, 1000);

        // Only 1 run
        assert_eq!(result.runs.len(), 1, "Expected to parse exactly one run");

        // Convenience
        let run = &result.runs[0];

        // Check actual benchmark info
        assert_eq!(run.execution_time_ms, 2138.223);
        assert_eq!(run.avg_ms, 2.138);
        assert_eq!(run.min_ms, 1.367);
        assert_eq!(run.max_ms, 11.710);

        let expected_ups = 1000.0 * 1000.0 / 2138.223; // ~467.67
        let difference = (run.effective_ups - expected_ups).abs();
        assert!(difference < 0.001, "Effective UPS calculation is incorrect");
    }
}
