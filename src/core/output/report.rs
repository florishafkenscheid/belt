use std::path::{Path, PathBuf};

use chrono::Local;
use handlebars::Handlebars;
use serde_json::json;

use crate::{
    benchmark::parser::BenchmarkResult,
    core::{
        error::{BenchmarkErrorKind, Result},
        output::{ResultWriter, WriteData, ensure_output_dir},
    },
};

pub struct ReportWriter {}

impl Default for ReportWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl ResultWriter for ReportWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Report {
                data,
                template_path,
            } => write_report(data, template_path, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()), // TODO
        }
    }
}

/// Write the results to a Handlebars file
fn write_report(
    results: &[BenchmarkResult],
    template_path: &Option<PathBuf>,
    path: &Path,
) -> Result<()> {
    ensure_output_dir(path)?;
    const TPL_STR: &str = "# Factorio Benchmark Results\n\n**Platform:** {{platform}}\n**Factorio Version:** {{factorio_version}}\n**Date:** {{date}}\n\n## Scenario\n* Each save was tested for {{ticks}} tick(s) and {{runs}} run(s)\n\n## Results\n| Metric            | Description                           |\n| ----------------- | ------------------------------------- |\n| **Mean UPS**      | Updates per second – higher is better |\n| **Mean Avg (ms)** | Average frame time – lower is better  |\n| **Mean Min (ms)** | Minimum frame time – lower is better  |\n| **Mean Max (ms)** | Maximum frame time – lower is better  |\n\n| Save | Avg (ms) | Min (ms) | Max (ms) | UPS | Execution Time (ms) | % Difference from base |\n|------|----------|----------|----------|-----|---------------------|------------------------|\n{{#each results}}\n| {{save_name}} | {{avg_ms}} | {{min_ms}} | {{max_ms}} | {{{avg_effective_ups}}} | {{total_execution_time_ms}} | {{percentage_improvement}} |\n{{/each}}\n\n## Conclusion";

    let mut handlebars = Handlebars::new();
    // Check for legacy path, otherwise use template string
    let results_path = if let Some(template_path) = template_path {
        let file_name = if template_path.extension().and_then(|s| s.to_str()) == Some("hbs") {
            template_path.file_stem().map(PathBuf::from).unwrap()
        } else {
            PathBuf::from("results.md")
        };

        handlebars.register_template_file("benchmark", template_path)?;

        path.join(file_name)
    } else {
        let legacy_path = PathBuf::from("templates/results.md.hbs");
        if legacy_path.exists() {
            handlebars.register_template_file("benchmark", legacy_path)?;
        } else {
            handlebars.register_template_string("benchmark", TPL_STR)?;
        }
        path.join("results.md")
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
