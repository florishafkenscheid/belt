//! Output utilities for BELT.
//!
//! Handles writing benchmark results to CSV and Markdown files, and manages report formatting.

use charming::ImageRenderer;
use handlebars::Handlebars;
use serde_json::json;
use std::path::Path;

use crate::{
    benchmark::{charts, parser::BenchmarkResult},
    core::{BenchmarkError, Result},
};

/// Create the specified directory, generate charts, and write the given results
pub async fn write_results(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: &Path,
    renderer: &mut ImageRenderer,
) -> Result<()> {
    write_csv(results, output_dir)?;
    if charts::generate_charts(results, output_dir, renderer)
        .await
        .is_ok()
    {
        write_markdown(results, output_dir, template_path)?;
    }

    Ok(())
}

/// Write the results to a CSV file
fn write_csv(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    let csv_path = output_dir.join("results.csv");

    let mut writer = csv::Writer::from_path(&csv_path).map_err(BenchmarkError::CsvError)?;

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

    writer.flush().map_err(BenchmarkError::IoError)?;
    tracing::info!("Results written to {}", csv_path.display());
    Ok(())
}

/// Write the results to a Markdown file
fn write_markdown(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: &Path,
) -> Result<()> {
    let results_path = output_dir.join(template_path.file_name().unwrap_or("results.md".as_ref()));
    let results_path = if results_path.extension().and_then(|s| s.to_str()) == Some("hbs") {
        &results_path.with_extension("")
    } else {
        &results_path
    };

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_file("benchmark", template_path)
        .map_err(|e| BenchmarkError::TemplateError(e.into()))?;

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
    });

    let rendered = handlebars
        .render("benchmark", &data)
        .map_err(BenchmarkError::TemplateError)?;

    std::fs::write(&results_path, rendered).map_err(BenchmarkError::IoError)?;

    tracing::info!("Report written to {}", results_path.display());
    Ok(())
}
