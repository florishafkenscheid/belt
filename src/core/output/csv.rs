use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Error, ErrorKind},
    path::Path,
};

use crate::{
    benchmark::{parser::BenchmarkRun, runner::VerboseData},
    core::{
        error::{BenchmarkErrorKind, Result},
        output::{ResultWriter, WriteData, ensure_output_dir},
    },
};

pub struct CsvWriter {}

impl Default for CsvWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl ResultWriter for CsvWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Benchmark(data) => write_benchmark_csv(data, path),
            WriteData::Verbose {
                data,
                metrics_to_export,
            } => write_verbose_csv(data, metrics_to_export, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }

    fn append(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Benchmark(data) => append_benchmark_csv(data, path),
            WriteData::Verbose {
                data,
                metrics_to_export,
            } => append_verbose_csv(data, metrics_to_export, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }
}

/// Write the results to a CSV file
fn write_benchmark_csv(results: &[BenchmarkRun], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    let csv_path = path.join("results.csv");

    let mut writer = csv::Writer::from_path(&csv_path)?;

    writer.write_record([
        "save_name",
        "run_index",
        "execution_time_ms",
        "avg_ms",
        "min_ms",
        "max_ms",
        "effective_ups",
        "percentage_improvement",
        "ticks",
        "factorio_version",
        "platform",
    ])?;

    for result in results {
        writer.write_record([
            &result.save_name,
            &result.index.to_string(),
            &result.execution_time_ms.to_string(),
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.effective_ups.to_string(),
            &result.base_diff.to_string(),
            &result.ticks.to_string(),
            &result.factorio_version,
            &result.platform,
        ])?;
    }

    writer.flush()?;
    tracing::info!("Results written to {}", csv_path.display());

    write_cpu_freq_csv(results, path)?;

    Ok(())
}

/// Write factorio's verbose output to a CSV file
fn write_verbose_csv(data: &[VerboseData], metrics: &[String], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    if data.is_empty() {
        return Ok(());
    }

    let csv_path = path.join(format!("{}_verbose_metrics.csv", data[0].save_name));
    let mut writer = csv::Writer::from_path(&csv_path)?;

    let first_run_csv_data = &data[0].csv_data;
    let mut reader = csv::Reader::from_reader(first_run_csv_data.as_bytes());
    let headers_from_factorio: Vec<String> =
        reader.headers()?.iter().map(|s| s.to_string()).collect();
    let header_map: HashMap<String, usize> = headers_from_factorio
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    let metrics_to_export: Vec<String> = if metrics.contains(&"all".to_string()) {
        headers_from_factorio
            .into_iter()
            .filter(|h| h != "tick" && h != "timestamp")
            .collect()
    } else {
        metrics.to_vec()
    };

    let mut header_row = vec!["tick".to_string(), "run".to_string()];
    header_row.extend(metrics_to_export.iter().cloned());
    writer.write_record(header_row)?;

    for (run_idx, run_data) in data.iter().enumerate() {
        let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
        // Skip headers
        let _ = inner_reader.headers()?;

        for record_result in inner_reader.records() {
            let record = record_result?;

            let tick_str = record.get(0).unwrap_or("t0");
            let tick_value = tick_str.trim_start_matches('t');

            let mut data_row = vec![tick_value.to_string(), run_idx.to_string()];

            for metric_name in &metrics_to_export {
                if let Some(&colum_index) = header_map.get(metric_name) {
                    let value = record.get(colum_index).unwrap_or("0");
                    data_row.push(value.to_string());
                } else {
                    data_row.push("N/A".to_string());
                }
            }
            writer.write_record(data_row)?;
        }
    }
    writer.flush()?;
    tracing::debug!(
        "Verbose metrics for {} exported to {}",
        data[0].save_name,
        csv_path.display()
    );
    Ok(())
}

fn write_cpu_freq_csv(data: &[BenchmarkRun], path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if let Some(first_run) = data.first()
        && first_run.cpu_data.is_empty()
    {
        tracing::debug!(
            "CPU frequency recording disabled or no CPU data captured. Skipping CPU frequency CSV."
        );
        return Ok(());
    }

    let csv_path = path.join("cpu_freq.csv");

    let mut writer = csv::Writer::from_path(&csv_path)?;

    writer.write_record([
        "save_name",
        "run_index",
        "core_index",
        "cpu_frequency",
        "timestamp",
    ])?;

    for result in data {
        for frequency_data in &result.cpu_data {
            writer.write_record([
                &result.save_name,
                &result.index.to_string(),
                &frequency_data.core_index.to_string(),
                &frequency_data.frequency.to_string(),
                &frequency_data.timestamp.to_string(),
            ])?;
        }
    }

    tracing::info!("CPU frequency results written to {}", csv_path.display());

    Ok(())
}

const BENCHMARK_HEADER: [&str; 11] = [
    "save_name",
    "run_index",
    "execution_time_ms",
    "avg_ms",
    "min_ms",
    "max_ms",
    "effective_ups",
    "percentage_improvement",
    "ticks",
    "factorio_version",
    "platform",
];

const CPU_FREQ_HEADER: [&str; 5] = [
    "save_name",
    "run_index",
    "core_index",
    "cpu_frequency",
    "timestamp",
];

fn append_benchmark_csv(results: &[BenchmarkRun], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    let csv_path = path.join("results.csv");
    if !csv_path.exists() {
        return write_benchmark_csv(results, path);
    }

    validate_csv_header(&csv_path, &BENCHMARK_HEADER)?;

    let next_indexes = next_benchmark_run_indexes(&csv_path)?;
    let adjusted_results = offset_benchmark_run_indexes(results, &next_indexes);

    let file = OpenOptions::new().append(true).open(&csv_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for result in &adjusted_results {
        writer.write_record([
            &result.save_name,
            &result.index.to_string(),
            &result.execution_time_ms.to_string(),
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.effective_ups.to_string(),
            &result.base_diff.to_string(),
            &result.ticks.to_string(),
            &result.factorio_version,
            &result.platform,
        ])?;
    }

    writer.flush()?;
    tracing::info!("Results appended to {}", csv_path.display());

    append_cpu_freq_csv(&adjusted_results, path)?;

    Ok(())
}

fn append_verbose_csv(data: &[VerboseData], metrics: &[String], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    if data.is_empty() {
        return Ok(());
    }

    let csv_path = path.join(format!("{}_verbose_metrics.csv", data[0].save_name));
    if !csv_path.exists() {
        return write_verbose_csv(data, metrics, path);
    }

    let first_run_csv_data = &data[0].csv_data;
    let mut reader = csv::Reader::from_reader(first_run_csv_data.as_bytes());
    let headers_from_factorio: Vec<String> =
        reader.headers()?.iter().map(|s| s.to_string()).collect();

    let header_map: HashMap<String, usize> = headers_from_factorio
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    let metrics_to_export: Vec<String> = if metrics.contains(&"all".to_string()) {
        headers_from_factorio
            .into_iter()
            .filter(|h| h != "tick" && h != "timestamp")
            .collect()
    } else {
        metrics.to_vec()
    };

    let mut expected_header = vec!["tick".to_string(), "run".to_string()];
    expected_header.extend(metrics_to_export.iter().cloned());

    validate_csv_header(&csv_path, &expected_header)?;

    let next_run_index = next_verbose_run_index(&csv_path)?;

    let file = OpenOptions::new().append(true).open(&csv_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for (local_run_idx, run_data) in data.iter().enumerate() {
        let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
        let _ = inner_reader.headers()?;

        let run_index = next_run_index + local_run_idx as u32;

        for record_result in inner_reader.records() {
            let record = record_result?;

            let tick_str = record.get(0).unwrap_or("t0");
            let tick_value = tick_str.trim_start_matches('t');

            let mut data_row = vec![tick_value.to_string(), run_index.to_string()];

            for metric_name in &metrics_to_export {
                if let Some(&column_index) = header_map.get(metric_name) {
                    let value = record.get(column_index).unwrap_or("0");
                    data_row.push(value.to_string());
                } else {
                    data_row.push("N/A".to_string());
                }
            }

            writer.write_record(data_row)?;
        }
    }

    writer.flush()?;
    tracing::debug!(
        "Verbose metrics for {} appended to {}",
        data[0].save_name,
        csv_path.display()
    );

    Ok(())
}

fn append_cpu_freq_csv(data: &[BenchmarkRun], path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if data.iter().all(|run| run.cpu_data.is_empty()) {
        return Ok(());
    }

    let csv_path = path.join("cpu_req.csv");
    if !csv_path.exists() {
        return write_cpu_freq_csv(data, path);
    }

    validate_csv_header(&csv_path, &CPU_FREQ_HEADER)?;

    let file = OpenOptions::new().append(true).open(&csv_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for result in data {
        for frequency_data in &result.cpu_data {
            writer.write_record([
                &result.save_name,
                &result.index.to_string(),
                &frequency_data.core_index.to_string(),
                &frequency_data.frequency.to_string(),
                &frequency_data.timestamp.to_string(),
            ])?;
        }
    }

    writer.flush()?;
    tracing::info!("CPU frequency results appended to {}", csv_path.display());

    Ok(())
}

fn validate_csv_header<S>(csv_path: &Path, expected: &[S]) -> Result<()>
where
    S: AsRef<str>,
{
    let mut reader = csv::Reader::from_path(csv_path)?;
    let actual = reader.headers()?;

    let expected: Vec<&str> = expected.iter().map(AsRef::as_ref).collect();

    if actual.iter().eq(expected.iter().copied()) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Cannot append to {}: header mismatch. Expected {:?}, found {:?}",
                csv_path.display(),
                expected,
                actual.iter().collect::<Vec<_>>()
            ),
        )
        .into())
    }
}

fn next_benchmark_run_indexes(csv_path: &Path) -> Result<HashMap<String, u32>> {
    let mut reader = csv::Reader::from_path(csv_path)?;
    let mut max_by_save: HashMap<String, u32> = HashMap::new();

    for record in reader.records() {
        let record = record?;
        let save_name = record.get(0).unwrap_or_default().to_string();
        let run_index = record.get(1).unwrap_or("0").parse::<u32>()?;

        max_by_save
            .entry(save_name)
            .and_modify(|max| *max = (*max).max(run_index))
            .or_insert(run_index);
    }

    Ok(max_by_save
        .into_iter()
        .map(|(save_name, max_index)| (save_name, max_index + 1))
        .collect())
}

fn offset_benchmark_run_indexes(
    results: &[BenchmarkRun],
    next_indexes: &HashMap<String, u32>,
) -> Vec<BenchmarkRun> {
    results
        .iter()
        .cloned()
        .map(|mut result| {
            let offset = next_indexes.get(&result.save_name).copied().unwrap_or(0);
            result.index += offset;
            result
        })
        .collect()
}

fn next_verbose_run_index(csv_path: &Path) -> Result<u32> {
    let mut reader = csv::Reader::from_path(csv_path)?;
    let mut max_run: Option<u32> = None;

    for record in reader.records() {
        let record = record?;
        let run_index = record.get(1).unwrap_or("0").parse::<u32>()?;
        max_run = Some(max_run.map_or(run_index, |max| max.max(run_index)));
    }

    Ok(max_run.map_or(0, |max| max + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::runner::CpuFrequencyData;

    #[test]
    fn test_cpu_freq_csv_uses_shared_filename_for_all_saves() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();

        let data = vec![
            BenchmarkRun {
                save_name: "alpha".to_string(),
                index: 0,
                cpu_data: vec![CpuFrequencyData {
                    frequency: 5000,
                    timestamp: 1,
                    core_index: 0,
                }],
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "beta".to_string(),
                index: 1,
                cpu_data: vec![CpuFrequencyData {
                    frequency: 5100,
                    timestamp: 2,
                    core_index: 1,
                }],
                ..Default::default()
            },
        ];

        write_cpu_freq_csv(&data, path).expect("write cpu csv");

        let csv_path = path.join("cpu_freq.csv");
        assert!(csv_path.exists(), "cpu_freq.csv should be created");

        let csv = std::fs::read_to_string(csv_path).expect("read cpu csv");
        assert!(csv.contains("alpha"));
        assert!(csv.contains("beta"));
    }
}
