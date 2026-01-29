use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::Local;
use handlebars::Handlebars;
use serde_json::json;

use crate::{
    benchmark::parser::BenchmarkRun,
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
            } => write_report(data, template_path.as_ref(), path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()), // TODO
        }
    }
}

/// Write the results to a Handlebars file
fn write_report(
    results: &[BenchmarkRun],
    template_path: Option<&PathBuf>,
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
    let aggs = aggregate_by_save_name(results);

    let mut table_results = Vec::new();
    for a in &aggs {
        let n = a.runs.max(1) as f64;

        let avg_ms = a.avg_ms / n;
        let avg_effective_ups = a.effective_ups / n;
        let avg_base_diff = a.base_diff / n;

        let min_ms = if a.min_ms.is_infinite() {
            0.0
        } else {
            a.min_ms
        };
        let max_ms = if a.max_ms.is_infinite() {
            0.0
        } else {
            a.max_ms
        };

        table_results.push(json!({
            "save_name": a.save_name,
            "avg_ms": format!("{:.3}", avg_ms),
            "min_ms": format!("{:.3}", min_ms),
            "max_ms": format!("{:.3}", max_ms),
            "avg_effective_ups": (avg_effective_ups as u64).to_string(),
            "percentage_improvement": format!("{:.2}%", avg_base_diff),
            "total_execution_time_ms": a.total_execution_time_ms as u64,
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
        "platform": results.first().map(|run| run.platform.as_str()),
        "factorio_version": results.first().map(|run| run.factorio_version.as_str()),
        "results": table_results,
        "ticks": results.first().map(|run| run.ticks).unwrap_or(0),
        "runs": results.len(),
        "date": Local::now().date_naive().to_string(),
    });

    let rendered = handlebars.render("benchmark", &data)?;

    std::fs::write(&results_path, rendered)?;

    tracing::info!("Report written to {}", results_path.display());
    Ok(())
}

#[derive(Debug, Clone)]
struct Aggregate {
    save_name: String,

    runs: u32,
    total_execution_time_ms: f64,
    avg_ms: f64,
    min_ms: f64,
    max_ms: f64,
    effective_ups: f64,
    base_diff: f64,
}

impl Aggregate {
    fn new(r: &BenchmarkRun) -> Self {
        Self {
            save_name: r.save_name.clone(),

            runs: 0,
            total_execution_time_ms: 0.0,
            avg_ms: 0.0,
            min_ms: f64::INFINITY,
            max_ms: f64::NEG_INFINITY,
            effective_ups: 0.0,
            base_diff: 0.0,
        }
    }

    fn push(&mut self, r: &BenchmarkRun) {
        self.runs += 1;
        self.total_execution_time_ms += r.execution_time_ms;

        self.avg_ms += r.avg_ms;
        self.min_ms = self.min_ms.min(r.min_ms);
        self.max_ms = self.max_ms.max(r.max_ms);

        self.effective_ups += r.effective_ups;
        self.base_diff += r.base_diff;
    }
}

fn aggregate_by_save_name(runs: &[BenchmarkRun]) -> Vec<Aggregate> {
    let mut map: HashMap<&str, Aggregate> = HashMap::new();

    for run in runs {
        map.entry(run.save_name.as_str())
            .or_insert_with(|| Aggregate::new(run))
            .push(run);
    }

    let mut aggs: Vec<Aggregate> = map.into_values().collect();
    aggs.sort_by(|a, b| a.save_name.cmp(&b.save_name));
    aggs
}
