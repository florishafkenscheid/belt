//! Parsing and aggregation of Factorio benchmark logs

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

use crate::benchmark::runner::CpuFrequencyData;
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
    pub mimalloc_stats: Option<MimallocStats>,
    pub cpu_data: Vec<CpuFrequencyData>,
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

static MIMALLOC_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"heap\sstats:\s*peak\s*total\s*current\s*block\s*total#\s*reserved:\s*(?P<reserved_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<reserved_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<reserved_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*committed:\s*(?P<committed_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<committed_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<committed_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*reset:\s*(?:\d+)\s*purged:\s*(?:\d+)\s*touched:\s*(?P<touched_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<touched_status>(?:[[:alpha:]]+[[:blank:]]?)*)\s*pages:\s*(?P<pages_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<pages_status>(?:[[:alpha:]]+[[:blank:]]?)*)\s*-abandoned:\s*(?P<abandoned_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<abandoned_status>(?:[[:alpha:]]+[[:blank:]]?)*).*\n.*\n.*\n.*\n.*\n.*\n.*\n.*\n.*\n\s*mmaps:\s*(?P<mmaps>\d+)\s*commits:\s*(?P<commits>\d+)\s*resets:\s*(?P<resets>\d+)\s*purges:\s*(?P<purges>\d+).*\n.*\s*threads:\s*(?P<threads_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_total>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_current>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?)\s*(?P<threads_status>(?:[[:alpha:]]+[[:blank:]]?)*)\n.*\n.*\n.*\n.*peak rss:\s(?P<rss_peak>(?:\d+)(?:\.\d+\s[[:alpha:]]{2,3})?).*"
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
            let ticks: u32 = get_capture(&captures, "ticks")?;
            let execution_time: f64 = get_capture(&captures, "execution_time")?;

            let effective_ups = 1000.0 * ticks as f64 / execution_time;

            run.ticks = ticks;
            run.execution_time_ms = execution_time;
            run.effective_ups = effective_ups;
        }

        if let Some(captures) = MS_REGEX.captures(line) {
            run.avg_ms = get_capture(&captures, "avg")?;
            run.min_ms = get_capture(&captures, "min")?;
            run.max_ms = get_capture(&captures, "max")?;
        }

        #[cfg(unix)]
        if line.contains("hugeadm:WARNING") {
            tracing::warn!("{line}");
        }
    }

    if let Some(start) = log.rfind("heap stats:")
        && let Some(captures) = MIMALLOC_REGEX.captures_at(log, start)
    {
        let committed_bytes = parse_size(get_capture(&captures, "committed_peak")?);
        let rss_bytes = parse_size(get_capture(&captures, "rss_peak")?);

        run.mimalloc_stats = Some(MimallocStats {
            committed_peak: get_capture(&captures, "committed_peak")?,
            peak_rss: get_capture(&captures, "rss_peak")?,
            reserved_peak: get_capture(&captures, "reserved_peak")?,
            committed_current: get_capture(&captures, "committed_current")?,
            reserved_current: get_capture(&captures, "reserved_current")?,
            pages_current: get_capture(&captures, "pages_current")?,
            pages_status: get_capture(&captures, "pages_status")?,
            abandoned_current: get_capture(&captures, "abandoned_current")?,
            abandoned_status: get_capture(&captures, "abandoned_status")?,
            threads_peak: get_capture(&captures, "threads_peak")?,
            threads_total: get_capture(&captures, "threads_total")?,
            mmaps: get_capture(&captures, "mmaps")?,
            purges: get_capture(&captures, "purges")?,
            resets: get_capture(&captures, "resets")?,
            commit_efficiency: format!(
                "{:.1}%",
                (rss_bytes as f64 / committed_bytes as f64) * 100.0
            ),
            thread_churn: get_capture::<u32>(&captures, "threads_total")?
                - get_capture::<u32>(&captures, "threads_peak")?,
        })
    }

    Ok(run)
}

fn get_capture<T>(captures: &Captures, key: &str) -> Result<T>
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

// Helper to parse "3.9 GiB" -> bytes
fn parse_size(s: String) -> u64 {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return 0;
    }
    let value = parts[0].parse::<f64>().unwrap_or(0.0);
    let multiplier = if parts.len() > 1 {
        match parts[1] {
            "KiB" | "KB" => 1024.0,
            "Ki" => 1024.0 * 8.0,
            "MiB" | "MB" => 1024.0 * 1024.0,
            "Mi" => 1024.0 * 1024.0 * 8.0,
            "GiB" | "GB" => 1024.0 * 1024.0 * 1024.0,
            "Gi" => 1024.0 * 1024.0 * 1024.0 * 8.0,
            "TiB" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            "Ti" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 8.0,
            _ => 1.0,
        }
    } else {
        1.0
    };
    (value * multiplier) as u64
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MimallocStats {
    pub committed_peak: String,
    pub peak_rss: String,
    pub reserved_peak: String,
    pub committed_current: String,
    pub reserved_current: String,
    pub pages_current: String,
    pub pages_status: String,
    pub abandoned_current: String,
    pub abandoned_status: String,
    pub threads_peak: u32,
    pub threads_total: u32,
    pub mmaps: String,
    pub purges: String,
    pub resets: String,
    pub commit_efficiency: String,
    pub thread_churn: u32,
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
