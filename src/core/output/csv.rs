use std::{collections::HashMap, path::Path};

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
