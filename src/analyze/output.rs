//! Output utilities for BELT.
//!
//! Handles writing benchmark results to CSV and Markdown files, and manages report formatting.

use charming::ImageRenderer;
use chrono::Local;
use handlebars::Handlebars;
use serde_json::json;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{
    benchmark::{charts, parser::BenchmarkResult, runner::VerboseData},
    core::Result,
};

/// Create the specified directory, generate charts, and write the given results
pub async fn write_results(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: Option<PathBuf>,
    renderer: &mut ImageRenderer,
) -> Result<()> {
    write_csv(results, output_dir)?;
    if charts::generate_charts(results, output_dir, renderer)
        .await
        .is_ok()
    {
        write_template(results, output_dir, template_path)?;
    }

    Ok(())
}

/// Write the results to a CSV file
fn write_csv(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    let csv_path = output_dir.join("results.csv");

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
        for (i, run) in result.runs.iter().enumerate() {
            writer.write_record([
                &result.save_name,
                &i.to_string(),
                &run.execution_time_ms.to_string(),
                &run.avg_ms.to_string(),
                &run.min_ms.to_string(),
                &run.max_ms.to_string(),
                &run.effective_ups.to_string(),
                &run.base_diff.to_string(),
                &result.ticks.to_string(),
                &result.factorio_version,
                &result.platform,
            ])?;
        }
    }

    writer.flush()?;
    tracing::info!("Results written to {}", csv_path.display());
    Ok(())
}

/// Write the results to a Markdown file
fn write_template(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: Option<PathBuf>,
) -> Result<()> {
    const TPL_STR: &str = "# Factorio Benchmark Results\n\n**Platform:** {{platform}}\n**Factorio Version:** {{factorio_version}}\n**Date:** {{date}}\n\n## Scenario\n* Each save was tested for {{ticks}} tick(s) and {{runs}} run(s)\n\n## Results\n| Metric            | Description                           |\n| ----------------- | ------------------------------------- |\n| **Mean UPS**      | Updates per second – higher is better |\n| **Mean Avg (ms)** | Average frame time – lower is better  |\n| **Mean Min (ms)** | Minimum frame time – lower is better  |\n| **Mean Max (ms)** | Maximum frame time – lower is better  |\n\n| Save | Avg (ms) | Min (ms) | Max (ms) | UPS | Execution Time (ms) | % Difference from base |\n|------|----------|----------|----------|-----|---------------------|------------------------|\n{{#each results}}\n| {{save_name}} | {{avg_ms}} | {{min_ms}} | {{max_ms}} | {{{avg_effective_ups}}} | {{total_execution_time_ms}} | {{percentage_improvement}} |\n{{/each}}\n\n![Chart](result_0_chart.svg)\n![Chart](result_1_chart.svg)\n![Chart](result_2_chart.svg)\n\n## Conclusion";

    let mut handlebars = Handlebars::new();
    let results_path = if let Some(template_path) = template_path {
        let file_name = if template_path.extension().and_then(|s| s.to_str()) == Some("hbs") {
            template_path.file_stem().map(PathBuf::from).unwrap()
        } else {
            PathBuf::from("results.md")
        };

        handlebars.register_template_file("benchmark", template_path)?;

        output_dir.join(file_name)
    } else {
        let legacy_path = PathBuf::from("templates/results.md.hbs");
        if legacy_path.exists() {
            handlebars.register_template_file("benchmark", legacy_path)?;
        } else {
            handlebars.register_template_string("benchmark", TPL_STR)?;
        }
        output_dir.join("results.md")
    };

    // Calculate aggregated metrics for each benchmark result
    let mut table_results = Vec::new();
    for result in results {
        // Aggregate metrics from all runs
        let total_execution_time_ms: f64 = result.runs.iter().map(|r| r.execution_time_ms).sum();
        let avg_ms: f64 =
            result.runs.iter().map(|r| r.avg_ms).sum::<f64>() / result.runs.len() as f64;

        let min_ms: f64 = result
            .runs
            .iter()
            .map(|r| r.min_ms)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let max_ms: f64 = result
            .runs
            .iter()
            .map(|r| r.max_ms)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let avg_effective_ups: f64 =
            result.runs.iter().map(|r| r.effective_ups).sum::<f64>() / result.runs.len() as f64;

        let avg_base_diff: f64 =
            result.runs.iter().map(|r| r.base_diff).sum::<f64>() / result.runs.len() as f64;

        // Round values for display
        let avg_ms_rounded = (avg_ms * 1000.0).round() / 1000.0;
        let min_ms_rounded = (min_ms * 1000.0).round() / 1000.0;
        let max_ms_rounded = (max_ms * 1000.0).round() / 1000.0;
        let avg_ups = avg_effective_ups as u64;

        table_results.push(json!({
            "save_name": result.save_name,
            "avg_ms": format!("{:.3}", avg_ms_rounded),
            "min_ms": format!("{:.3}", min_ms_rounded),
            "max_ms": format!("{:.3}", max_ms_rounded),
            "avg_effective_ups": avg_ups.to_string(),
            "percentage_improvement": format!("{:.2}%", avg_base_diff),
            "total_execution_time_ms": total_execution_time_ms as u64,
        }));
    }

    let bolding_tags = match results_path.extension().and_then(|s| s.to_str()) {
        Some("html") => ("<strong>", "</strong>"),
        Some("md") => ("**", "**"),
        _ => ("**", "**"),
    };

    // Find the highest avg_effective_ups across all benchmarks for highlighting
    if !table_results.is_empty() {
        let max_avg_ups = table_results
            .iter()
            .map(|r| {
                r["avg_effective_ups"]
                    .as_str()
                    .unwrap_or("0")
                    .parse::<u64>()
                    .unwrap_or(0)
            })
            .max()
            .unwrap_or(0);

        // Add bold formatting to the highest UPS value
        for result in &mut table_results {
            let ups_str = result["avg_effective_ups"].as_str().unwrap_or("0");
            let ups = ups_str.parse::<u64>().unwrap_or(0);
            if ups == max_avg_ups {
                result["avg_effective_ups"] =
                    json!(format!("{}{}{}", bolding_tags.0, ups, bolding_tags.1));
            }
        }
    }

    let data = json!({
        "platform": results.first().map(|r| &r.platform).unwrap_or(&"unknown".to_string()).to_string(),
        "factorio_version": results.first().map(|r| &r.factorio_version).unwrap_or(&"unknown".to_string()).to_string(),
        "results": table_results,
        "ticks": results.first().map(|r| r.ticks).unwrap_or(0),
        "runs": results.first().map(|r| r.runs.len()).unwrap_or(0),
        "date": Local::now().date_naive().to_string(),
    });

    let rendered = handlebars.render("benchmark", &data)?;

    std::fs::write(&results_path, rendered)?;

    tracing::info!("Report written to {}", results_path.display());
    Ok(())
}

pub fn write_verbose_metrics_csv(
    save_name: &str,
    save_verbose_data: &[VerboseData],
    metrics_to_export: &[String],
    output_dir: &Path,
) -> Result<()> {
    if save_verbose_data.is_empty() {
        return Ok(());
    }

    let csv_path = output_dir.join(format!("{save_name}_verbose_metrics.csv"));
    let mut writer = csv::Writer::from_path(&csv_path)?;

    let first_run_csv_data = &save_verbose_data[0].csv_data;
    let mut reader = csv::Reader::from_reader(first_run_csv_data.as_bytes());
    let headers_from_factorio: Vec<String> =
        reader.headers()?.iter().map(|s| s.to_string()).collect();
    let header_map: HashMap<String, usize> = headers_from_factorio
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    let actual_metrics_to_export: Vec<String> = if metrics_to_export.contains(&"all".to_string()) {
        headers_from_factorio
            .into_iter()
            .filter(|h| h != "tick" && h != "timestamp")
            .collect()
    } else {
        metrics_to_export.to_vec()
    };

    let mut header_row = vec!["tick".to_string(), "run".to_string()];
    header_row.extend(actual_metrics_to_export.iter().cloned());
    writer.write_record(header_row)?;

    for (run_idx, run_data) in save_verbose_data.iter().enumerate() {
        let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
        // Skip headers
        let _ = inner_reader.headers()?;

        for record_result in inner_reader.records() {
            let record = record_result?;

            let tick_str = record.get(0).unwrap_or("t0");
            let tick_value = tick_str.trim_start_matches('t');

            let mut data_row = vec![tick_value.to_string(), run_idx.to_string()];

            for metric_name in &actual_metrics_to_export {
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
    tracing::info!(
        "Verbose metrics for {} exported to {}",
        save_name,
        csv_path.display()
    );
    Ok(())
}
