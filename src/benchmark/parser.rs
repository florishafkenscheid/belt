//! Parsing and aggregation of Factorio benchmark logs

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

use crate::core::config::BenchmarkConfig;
use crate::core::error::BenchmarkError;
use crate::core::error::BenchmarkErrorKind;
use crate::core::{Result, get_os_info};

/// The result of a benchmark of a single run
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkRun {
    pub index: u32,
    pub save_name: String,
    pub factorio_version: String,
    pub platform: String,
    pub execution_time_ms: f64,
    pub ticks: u32,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub effective_ups: f64,
    pub base_diff: f64,
}

// Build perfomance line regexs
static PERFORMED_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*Performed\s*(?P<ticks>[0-9]+)\s*updates\s*in\s*(?P<execution_time>[0-9]+(?:\.[0-9]+)?)\s*ms$"
    ).expect("Regex building failed")
});

static MS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*avg:\s*(?P<avg>[0-9]+(?:\.[0-9]+)?)\s*ms,\s*min:\s*(?P<min>[0-9]+(?:\.[0-9]+)?)\s*ms,\s*max:\s*(?P<max>[0-9]+(?:\.[0-9]+)?)\s*ms\s*$",
    ).expect("Regex building failed")
});

/// Parsing of the given Factorio output
pub fn parse_benchmark_log(
    log: &str,
    save_file: &Path,
    benchmark_config: &BenchmarkConfig,
) -> Result<BenchmarkRun> {
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
    let iterator = log.lines().peekable();

    // Create run to write into
    let mut run = BenchmarkRun {
        save_name,
        factorio_version: version,
        platform: get_os_info(),
        ..Default::default()
    };

    // Iterate over collected lines
    for line in iterator {
        if let Some(captures) = PERFORMED_REGEX.captures(line) {
            let ticks: u32 = get_named_type(&captures, "ticks")?;
            let execution_time: f64 = get_named_type(&captures, "execution_time")?;

            let effective_ups = 1000.0 * ticks as f64 / execution_time;

            run.ticks = ticks;
            run.execution_time_ms = execution_time;
            run.effective_ups = effective_ups;
        }

        if let Some(captures) = MS_REGEX.captures(line) {
            run.avg_ms = get_named_type(&captures, "avg")?;
            run.min_ms = get_named_type(&captures, "min")?;
            run.max_ms = get_named_type(&captures, "max")?;
        }
    }

    Ok(run)
}

fn get_named_type<T>(captures: &Captures, key: &str) -> Result<T>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let s = captures
        .name(key)
        .ok_or_else(|| {
            BenchmarkError::from(BenchmarkErrorKind::MissingCaptureField {
                field: key.to_string(),
            })
        })?
        .as_str();

    s.parse::<T>().map_err(|_| {
        BenchmarkError::from(BenchmarkErrorKind::MalformedBenchmarkOutput {
            field: key.to_string(),
            string: s.to_string(),
        })
    })
}

#[cfg(test)]
mod tests {
    use crate::core::utils;

    use super::*;

    #[test]
    fn test_calculate_base_differences_simple() {
        let mut results = vec![
            BenchmarkRun {
                save_name: "base_save".to_string(),
                effective_ups: 50.0,
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "fast_save".to_string(),
                effective_ups: 100.0,
                ..Default::default()
            },
        ];

        utils::calculate_base_differences(&mut results);

        assert_eq!(
            results[0].base_diff, 0.0,
            "The worst-performing save should have 0% improvement"
        );
        assert_eq!(
            results[1].base_diff, 100.0,
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

        // Check actual benchmark info
        assert_eq!(result.execution_time_ms, 2138.223);
        assert_eq!(result.avg_ms, 2.138);
        assert_eq!(result.min_ms, 1.367);
        assert_eq!(result.max_ms, 11.710);

        let expected_ups = 1000.0 * 1000.0 / 2138.223; // ~467.67
        let difference = (result.effective_ups - expected_ups).abs();
        assert!(difference < 0.001, "Effective UPS calculation is incorrect");
    }
}
