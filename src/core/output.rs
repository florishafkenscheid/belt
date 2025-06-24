use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;
use std::path::Path;

use crate::benchmark::parser::BenchmarkResult;

pub fn write_results(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: &Path,
) -> Result<()> {
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create the output directory: {}",
            output_dir.display()
        )
    })?;

    write_csv(results, output_dir)?;
    write_markdown(results, output_dir, template_path)?;

    Ok(())
}

fn write_csv(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    let csv_path = output_dir.join("results.csv");

    let mut writer = csv::Writer::from_path(&csv_path).context("Failed to create CSV writer")?;

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

    writer.flush().context("Failed to write CSV")?;
    tracing::info!("Results written to {}", csv_path.display());
    Ok(())
}

fn write_markdown(
    results: &[BenchmarkResult],
    output_dir: &Path,
    template_path: &Path,
) -> Result<()> {
    let md_path = output_dir.join("results.md");

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_file("benchmark", template_path)
        .context("Failed to register template")?;

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

        // Calculate percentage improvement from base (worst performer)
        let base_ups = avg_effective_ups - avg_base_diff;
        let percentage_improvement = if base_ups > 0.0 {
            (avg_base_diff / base_ups) * 100.0
        } else {
            0.0
        };

        table_results.push(json!({
            "save_name": result.save_name,
            "avg_ms": format!("{:.3}", avg_ms_rounded),
            "min_ms": format!("{:.3}", min_ms_rounded),
            "max_ms": format!("{:.3}", max_ms_rounded),
            "avg_effective_ups": avg_ups.to_string(),
            "percentage_improvement": format!("{:.2}%", percentage_improvement),
            "total_execution_time_ms": total_execution_time_ms as u64,
        }));
    }

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
                result["avg_effective_ups"] = json!(format!("**{}**", ups));
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
        .context("Failed to render template")?;

    std::fs::write(&md_path, rendered).context("Failed to write markdown file")?;

    tracing::info!("Markdown report written to {}", md_path.display());
    Ok(())
}
