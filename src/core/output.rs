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
        "run",
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
            &result.run.to_string(),
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.ticks.to_string(),
            &result.execution_time_ms.to_string(),
            &result.effective_ups.to_string(),
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
    md_path :&Path,
    template_path: &Path,
) -> Result<()> {
    let mut handlebars = Handlebars::new();

    handlebars.register_template_file("benchmark.md.hbs", template_path)
        .context("Failed to register template")?;

    use std::collections::HashMap;
    let mut grouped: HashMap<String, Vec<&BenchmarkResult>> = HashMap::new();

    for result in results {
        grouped.entry(result.save_name.clone())
            .or_default()
            .push(result);
    }

    let mut summaries = Vec::new();
    for (save_name, save_results) in grouped {
        let avg_ups: f64 = save_results.iter()
            .map(|r| r.effective_ups)
            .sum::<f64>() / save_results.len() as f64;

        let min_ups = save_results.iter()
            .map(|r| r.effective_ups)
            .fold(f64::INFINITY, f64::min);

        let max_ups = save_results.iter()
            .map(|r| r.effective_ups)
            .fold(f64::NEG_INFINITY, f64::max);

        summaries.push(json!({
            "save_name": save_name,
            "runs": save_results,
            "avg_ups": format!("{:.3}", avg_ups),
            "min_ups": format!("{:.3}", min_ups),
            "max_ups": format!("{:.3}", max_ups),
            "run_count": save_results.len()
        }));
    }

    let data = json!({
        "results": results,
        "summaries": summaries,
        "total_runs": results.len(),
        "platform": results.first().map(|r| &r.platform).unwrap_or(&"unknown".to_string()).to_string(),
        "factorio_version": results.first().map(|r| &r.factorio_version).unwrap_or(&"unknown".to_string()).to_string()
    });

    let rendered = handlebars.render("benchmark.md.hbs", &data)
        .context("Failed to render template")?;

    std::fs::write(md_path, rendered)
        .context("Failed to write markdown file")?;
        
    tracing::info!("Markdown report written to {}", md_path.display());
    Ok(())
}
