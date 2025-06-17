use std::path::Path;
use anyhow::{Context, Result};
use handlebars::Handlebars;
use serde_json::json;

use crate::benchmark::parser::BenchmarkResult;

pub fn write_results(
    results: &[BenchmarkResult],
    csv_path: &Path,
    md_path: &Path,
    template_path: &Path
) -> Result<()> {
    write_csv(results, csv_path)?;
    write_markdown(results, md_path, template_path)?;
    Ok(())
}

fn write_csv(results: &[BenchmarkResult], path: &Path) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .context("Failed to create CSV writer")?;

    writer.write_record(&[
        "save_name",
        "avg_ms",
        "min_ms",
        "max_ms",
        "ticks",
        "execution_time_ms",
        "effective_ups",
        "factorio_version",
        "platform",
    ])?;

    for result in results {
        writer.write_record(&[
            &result.save_name,
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.ticks.to_string(),
            &result.total_execution_time_ms.to_string(),
            &result.avg_effective_ups.to_string(),
            &result.factorio_version,
            &result.platform,
        ])?;
    }

    writer.flush().context("Failed to write CSV")?;
    tracing::info!("Results written to {}", path.display());
    Ok(())
}

fn write_markdown(
    results: &[BenchmarkResult],
    md_path: &Path,
    template_path: &Path,
) -> Result<()> {
    let mut handlebars = Handlebars::new();

    handlebars.register_template_file("benchmark", template_path)
        .context("Failed to register template")?;

    // Find the highest avg_effective_ups across all benchmarks
    let max_avg_ups = results.iter()
        .map(|r| r.avg_effective_ups as u64)
        .max()
        .unwrap_or(0);

    // Prepare results for the table with bold formatting
    let mut table_results = Vec::new();
    for result in results {
        let avg_ups = result.avg_effective_ups as u64;
        let avg_ms_rounded = (result.avg_ms * 1000.0).round() / 1000.0;
        let min_ms_rounded = (result.min_ms * 1000.0).round() / 1000.0;
        let max_ms_rounded = (result.max_ms * 1000.0).round() / 1000.0;

        table_results.push(json!({
            "save_name": result.save_name,
            "avg_ms": format!("{:.3}", avg_ms_rounded),
            "min_ms": format!("{:.3}", min_ms_rounded),
            "max_ms": format!("{:.3}", max_ms_rounded),
            "avg_effective_ups": if avg_ups == max_avg_ups { format!("**{}**", avg_ups) } else { avg_ups.to_string() },
            "total_execution_time_ms": result.total_execution_time_ms,
        }));
    }

    let data = json!({
        "platform": results.first().map(|r| &r.platform).unwrap_or(&"unknown".to_string()).to_string(),
        "factorio_version": results.first().map(|r| &r.factorio_version).unwrap_or(&"unknown".to_string()).to_string(),
        "results": table_results,
    });

    let rendered = handlebars.render("benchmark.md.hbs", &data)
        .context("Failed to render template")?;

    std::fs::write(md_path, rendered)
        .context("Failed to write markdown file")?;
        
    tracing::info!("Markdown report written to {}", md_path.display());
    Ok(())
}
