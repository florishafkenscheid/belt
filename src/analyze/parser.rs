use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use csv::Reader;

use crate::{
    benchmark::{
        parser::{BenchmarkResult, BenchmarkRun},
        runner::VerboseData,
    },
    core::{
        error::{BenchmarkErrorKind, Result},
        utils,
    },
};

/// Read both results.csv and *_verbose_metrics.csv and reconstruct the data therein
#[allow(clippy::complexity)]
pub fn read_data(
    data_dir: &Path,
) -> Result<(Vec<BenchmarkResult>, HashMap<String, Vec<VerboseData>>)> {
    let files = utils::find_data_files(data_dir)?;

    let results_csv = files
        .iter()
        .find(|file| file.file_name().and_then(|name| name.to_str()) == Some("results.csv"))
        .ok_or_else(|| BenchmarkErrorKind::DataFileNotFound {
            path: data_dir.join("results.csv"),
        })?;

    let verbose_files: Vec<PathBuf> = files
        .iter()
        .filter(|file| {
            file.file_name()
                .and_then(|name| name.to_str())
                .map(|string| string.ends_with("_verbose_metrics.csv"))
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let results = read_benchmark_results(results_csv)?;
    let verbose_data = read_verbose_data(&verbose_files)?;

    Ok((results, verbose_data))
}

/// Read results.csv and reconstruct BenchmarkResult vector
fn read_benchmark_results(csv_path: &PathBuf) -> Result<Vec<BenchmarkResult>> {
    let mut reader = Reader::from_path(csv_path)?;
    let mut results_map: HashMap<String, BenchmarkResult> = HashMap::new();

    for result in reader.records() {
        let record = result?;

        let save_name = record.get(0).unwrap_or("unknown").to_string();
        let run_index: usize = record.get(1).unwrap_or("0").parse().unwrap_or(0);
        let execution_time_ms: f64 = record.get(2).unwrap_or("0").parse().unwrap_or(0.0);
        let avg_ms: f64 = record.get(3).unwrap_or("0").parse().unwrap_or(0.0);
        let min_ms: f64 = record.get(4).unwrap_or("0").parse().unwrap_or(0.0);
        let max_ms: f64 = record.get(5).unwrap_or("0").parse().unwrap_or(0.0);
        let effective_ups: f64 = record.get(6).unwrap_or("0").parse().unwrap_or(0.0);
        let base_diff: f64 = record.get(7).unwrap_or("0").parse().unwrap_or(0.0);
        let ticks: u32 = record.get(8).unwrap_or("0").parse().unwrap_or(0);
        let factorio_version = record.get(9).unwrap_or("unknown").to_string();
        let platform = record.get(10).unwrap_or("unknown").to_string();

        let run = BenchmarkRun {
            execution_time_ms,
            avg_ms,
            min_ms,
            max_ms,
            effective_ups,
            base_diff,
        };

        let result = results_map
            .entry(save_name.clone())
            .or_insert_with(|| BenchmarkResult {
                save_name,
                ticks,
                runs: Vec::new(),
                factorio_version,
                platform,
            });

        if result.runs.len() <= run_index {
            result.runs.resize(run_index + 1, BenchmarkRun::default());
        }
        result.runs[run_index] = run;
    }

    tracing::debug!("Read results from: {}", csv_path.display());

    Ok(results_map.into_values().collect())
}

/// Read *_verbose_metrics.csv files and reconstruct VerboseData
fn read_verbose_data(verbose_csv_files: &[PathBuf]) -> Result<HashMap<String, Vec<VerboseData>>> {
    let mut verbose_data_by_save: HashMap<String, Vec<VerboseData>> = HashMap::new();

    for csv_path in verbose_csv_files {
        let file_stem = csv_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .strip_suffix("_verbose_metrics")
            .unwrap_or("")
            .to_string();

        let mut reader = Reader::from_path(csv_path)?;
        let headers = reader.headers()?.clone();

        let mut runs_data: HashMap<usize, String> = HashMap::new();

        for result in reader.records() {
            let record = result?;
            let run_index: usize = record.get(1).unwrap_or("0").parse().unwrap_or(0);

            let entry = runs_data.entry(run_index).or_insert_with(|| {
                let mut csv_content = String::new();
                let original_headers: Vec<&str> = headers.iter().skip(2).collect();
                csv_content.push_str(&original_headers.join(","));
                csv_content.push('\n');
                csv_content
            });

            let data_values: Vec<&str> = record.iter().skip(2).collect();
            entry.push_str(&format!("{}\n", data_values.join(",")));
        }

        let mut verbose_data: Vec<VerboseData> = runs_data
            .into_iter()
            .map(|(run_index, csv_data)| VerboseData {
                save_name: file_stem.clone(),
                run_index,
                csv_data,
            })
            .collect();

        verbose_data.sort_by_key(|vd| vd.run_index);
        verbose_data_by_save.insert(file_stem, verbose_data);
    }

    tracing::debug!("Read data from:");
    for file in verbose_csv_files {
        tracing::debug!("  - {}", file.display())
    }

    Ok(verbose_data_by_save)
}
