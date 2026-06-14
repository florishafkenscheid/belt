use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::Local;
use handlebars::Handlebars;
use serde_json::json;

use crate::{
    benchmark::{
        parser::{BenchmarkRun, MimallocStats},
        uprof::{self, AmdUprofParsedReport, AmdUprofReportArtifact},
    },
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
            } => write_report(data, template_path.as_deref(), path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()), // TODO
        }
    }
}

/// Write the results to a Handlebars file
fn write_report(results: &[BenchmarkRun], template_path: Option<&Path>, path: &Path) -> Result<()> {
    const TPL_STR: &str = "# Factorio Benchmark Results\n\n**Platform:** {{platform}}\n**Factorio Version:** {{factorio_version}}\n**Date:** {{date}}\n\n## Scenario\n* Each save was tested for {{ticks}} tick(s) and {{runs}} run(s)\n\n## Results\n| Metric            | Description                           |\n| ----------------- | ------------------------------------- |\n| **Mean UPS**      | Updates per second – higher is better |\n| **Mean Avg (ms)** | Average frame time – lower is better  |\n| **Mean Min (ms)** | Minimum frame time – lower is better  |\n| **Mean Max (ms)** | Maximum frame time – lower is better  |\n\n| Save | Avg (ms) | Min (ms) | Max (ms) | UPS | Execution Time (ms) | % Difference from base |\n|------|----------|----------|----------|-----|---------------------|------------------------|\n{{#each results}}\n| {{save_name}} | {{avg_ms}} | {{min_ms}} | {{max_ms}} | {{{avg_effective_ups}}} | {{total_execution_time_ms}} | {{percentage_improvement}} |\n{{/each}}\n\n{{#if results.0.mimalloc}}\n## Memory (mimalloc)\n\n### What these numbers mean (practical interpretation)\n| Field | What it roughly indicates |\n|------|----------------------------|\n| **Committed (peak)** | Highest amount of memory backed by the OS during the run (best \"memory footprint\" trend metric). |\n| **Reserved (peak)** | Highest virtual address space reserved by the allocator. **If Committed > Reserved, the application uses direct `mmap`/`VirtualAlloc` outside the allocator** (e.g., for memory-mapped files or custom pools). |\n| **Peak RSS** | Highest resident set size (what was actually in RAM). Large gaps between Committed and RSS indicate sparse memory usage (hugepages, memory-mapped files, or reserved-but-untouched arenas). |\n| **Commit Efficiency** | `(Peak RSS / Committed Peak)` as percentage. <10% = sparse allocation (mostly reserved, not touched); >80% = dense working set. |\n| **Committed/Reserved (current)** | What the allocator still held at process exit. Not automatically a leak—mimalloc retains arenas for reuse. **Trend this across multiple runs; growth between identical runs indicates leaks.** |\n| **Pages / Abandoned (current + status)** | \"Not all freed\" is **normal**—the allocator caches pages for reuse. Abandoned blocks indicate thread-local heap fragments from terminated threads. Flag only if these numbers grow across benchmark iterations. |\n| **Thread Churn** | `(Threads Peak - Current)`. Values >0 indicate short-lived worker threads spawned during initialization (explains Abandoned blocks). |\n| **Threads (peak)** | Peak allocator thread count observed. If Peak > Current, expect elevated Abandoned blocks. |\n| **mmaps** | Number of OS allocation calls. Low counts (<50) with high memory usage indicate efficient arena reuse. High counts indicate frequent allocation pressure or fragmentation. |\n| **purges / resets** | Memory returned to OS. Usually 0 in benchmarks—non-zero indicates aggressive memory trimming or constrained environments. |\n\n### Summary (end-of-run heap stats)\n| Save | Committed Peak | Peak RSS | Commit Efficiency | Reserved Peak | Committed Current | Reserved Current | Pages Current | Pages Status | Abandoned Current | Abandoned Status | Thread Churn | Threads Peak | mmaps | purges | resets |\n|------|----------------|----------|-------------------|---------------|-------------------|------------------|---------------|-------------|-------------------|------------------|--------------|-------------|-------|--------|--------|\n{{#each results}}\n{{#each mimalloc}}\n| {{../save_name}} | {{committed_peak}} | {{peak_rss}} | {{commit_efficiency}} | {{reserved_peak}} | {{committed_current}} | {{reserved_current}} | {{pages_current}} | {{pages_status}} | {{abandoned_current}} | {{abandoned_status}} | {{thread_churn}} | {{threads_peak}} | {{mmaps}} | {{purges}} | {{resets}} |\n{{/each}}\n{{/each}}\n\n{{/if}}\n{{{amd_uprof_markdown}}}\n## Conclusion";
    ensure_output_dir(path)?;

    let mut report_results = results.to_vec();
    for run in &mut report_results {
        uprof::archive_and_parse_run(run, path);
    }

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
    let aggs = aggregate_by_save_name(&report_results);
    let amd_uprof_markdown = render_amd_uprof_markdown(&report_results, path);

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
            "mimalloc": a.mimalloc_stats,
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
        "ticks": report_results.first().map(|run| run.ticks).unwrap_or(0),
        "runs": aggs.first().map(|aggregate| aggregate.runs).unwrap_or(0),
        "date": Local::now().date_naive().to_string(),
        "amd_uprof_markdown": amd_uprof_markdown,
    });

    let rendered = handlebars.render("benchmark", &data)?;

    std::fs::write(&results_path, rendered)?;

    tracing::info!("Report written to {}", results_path.display());
    Ok(())
}

fn render_amd_uprof_markdown(results: &[BenchmarkRun], output_dir: &Path) -> String {
    let detected = results
        .iter()
        .filter(|run| run.amd_uprof.is_some())
        .collect::<Vec<_>>();

    if detected.is_empty() {
        return String::new();
    }

    let mut markdown = String::from("## AMD uProf\n\n");
    markdown.push_str("| Save | Run | Profile | View | Duration | Threads | Session | Report |\n");
    markdown.push_str("|------|-----|---------|------|----------|---------|---------|--------|\n");

    for run in &detected {
        let uprof = run.amd_uprof.as_ref().expect("checked above");

        if uprof.reports.is_empty() {
            for session in &uprof.session_paths {
                markdown.push_str(&format!(
                    "| {} | {} |  |  |  |  | {} | Run `{}` |\n",
                    markdown_cell(&run.save_name),
                    run.index,
                    markdown_cell(&display_path(session, output_dir)),
                    markdown_cell(&format!("AMDuProfCLI report -i {}", session.display())),
                ));
            }
            continue;
        }

        for report in &uprof.reports {
            let parsed = report.parsed.as_ref();
            markdown.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                markdown_cell(&run.save_name),
                run.index,
                markdown_cell(metadata_or_empty(parsed, "Profile Session Type")),
                markdown_cell(metadata_or_empty(parsed, "Selected View")),
                markdown_cell(metadata_or_empty(parsed, "Profile Duration")),
                markdown_cell(metadata_or_empty(parsed, "Thread Count")),
                markdown_cell(
                    &uprof
                        .session_paths
                        .last()
                        .map(|path| display_path(path, output_dir))
                        .unwrap_or_default()
                ),
                markdown_cell(&report_path_cell(report, output_dir)),
            ));
        }
    }

    for run in detected {
        let uprof = run.amd_uprof.as_ref().expect("checked above");
        for (report_index, report) in uprof.reports.iter().enumerate() {
            markdown.push_str(&format!(
                "\n### {} / run_{} / report_{}\n\n",
                run.save_name, run.index, report_index
            ));

            if let Some(error) = &report.copy_error {
                markdown.push_str(&format!(
                    "Report archive warning: {}\n\n",
                    markdown_text(error)
                ));
            }

            if let Some(error) = &report.parse_error {
                markdown.push_str(&format!(
                    "Report parse warning: {}. Full CSV: `{}`\n\n",
                    markdown_text(error),
                    markdown_text(&report_path_cell(report, output_dir))
                ));
            }

            let Some(parsed) = &report.parsed else {
                continue;
            };

            render_report_metadata(&mut markdown, parsed);
            render_cache_summary(&mut markdown, parsed);

            for table in &parsed.tables {
                markdown.push_str(&format!("#### {}\n\n", markdown_text(&table.title)));
                render_markdown_table(&mut markdown, &table.headers, &table.rows);
                if table.truncated {
                    markdown.push_str(&format!(
                        "This AMD uProf table was truncated in Markdown. Full CSV: `{}`\n\n",
                        markdown_text(&report_path_cell(report, output_dir))
                    ));
                }
            }

            if parsed.truncated {
                markdown.push_str(&format!(
                    "This AMD uProf report was truncated in Markdown. Full CSV: `{}`\n\n",
                    markdown_text(&report_path_cell(report, output_dir))
                ));
            }
        }
    }

    markdown.push('\n');
    markdown
}

fn render_report_metadata(markdown: &mut String, parsed: &AmdUprofParsedReport) {
    let promoted = [
        "Profile Session Type",
        "Selected View",
        "Profile Duration",
        "Thread Count",
        "Data Folder",
    ];

    let rows = promoted
        .iter()
        .filter_map(|key| parsed.metadata_value(key).map(|value| (*key, value)))
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return;
    }

    markdown.push_str("| Field | Value |\n");
    markdown.push_str("|-------|-------|\n");
    for (key, value) in rows {
        markdown.push_str(&format!(
            "| {} | {} |\n",
            markdown_cell(key),
            markdown_cell(value)
        ));
    }
    markdown.push('\n');
}

fn render_markdown_table(markdown: &mut String, headers: &[String], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }

    markdown.push('|');
    for header in headers {
        markdown.push(' ');
        markdown.push_str(&markdown_cell(header));
        markdown.push_str(" |");
    }
    markdown.push('\n');

    markdown.push('|');
    for _ in headers {
        markdown.push_str("------|");
    }
    markdown.push('\n');

    for row in rows {
        markdown.push('|');
        for index in 0..headers.len() {
            markdown.push(' ');
            markdown.push_str(&markdown_cell(
                row.get(index).map(String::as_str).unwrap_or(""),
            ));
            markdown.push_str(" |");
        }
        markdown.push('\n');
    }
    markdown.push('\n');
}

fn render_cache_summary(markdown: &mut String, parsed: &AmdUprofParsedReport) {
    let rows = parsed
        .tables
        .iter()
        .filter_map(|table| cache_table_indexes(table).map(|indexes| (table, indexes)))
        .flat_map(|(table, indexes)| {
            table.rows.iter().filter_map(move |row| {
                let accesses = parse_metric(row.get(indexes.accesses)?)?;
                let local_l2 = parse_metric(row.get(indexes.local_l2)?)?;
                let local_cache = parse_metric(row.get(indexes.local_cache)?)?;
                let external_cache = parse_metric(row.get(indexes.external_cache)?)?;
                let local_dram = parse_metric(row.get(indexes.local_dram)?)?;
                let misses = local_l2 + local_cache + external_cache + local_dram;
                let hits = (accesses - misses).max(0.0);
                let miss_rate = if accesses > 0.0 {
                    misses / accesses * 100.0
                } else {
                    0.0
                };

                Some(CacheSummaryRow {
                    table: table.title.clone(),
                    item: row.first().cloned().unwrap_or_default(),
                    accesses,
                    hits,
                    misses,
                    miss_rate,
                    local_l2,
                    local_cache,
                    external_cache,
                    local_dram,
                })
            })
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        return;
    }

    markdown.push_str("#### Estimated L1 Data Cache Summary\n\n");
    markdown.push_str(
        "Estimated from `L1_DC_ACCESSES_ALL.USER` and demand refill source counters.\n\n",
    );
    markdown.push_str("| Table | Item | Accesses | Est Hits | Est Misses | Est Miss Rate | L2 Refills | Cache Refills | External Cache Refills | DRAM Refills |\n");
    markdown.push_str("|-------|------|----------|----------|------------|---------------|------------|---------------|------------------------|--------------|\n");

    for row in rows {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.2}% | {} | {} | {} | {} |\n",
            markdown_cell(&row.table),
            markdown_cell(&row.item),
            format_metric(row.accesses),
            format_metric(row.hits),
            format_metric(row.misses),
            row.miss_rate,
            format_metric(row.local_l2),
            format_metric(row.local_cache),
            format_metric(row.external_cache),
            format_metric(row.local_dram),
        ));
    }
    markdown.push('\n');
}

fn metadata_or_empty<'a>(parsed: Option<&'a AmdUprofParsedReport>, key: &str) -> &'a str {
    parsed
        .and_then(|parsed| parsed.metadata_value(key))
        .unwrap_or("")
}

fn report_path_cell(report: &AmdUprofReportArtifact, output_dir: &Path) -> String {
    let path = report.copied_path.as_ref().unwrap_or(&report.original_path);
    display_path(path, output_dir)
}

fn display_path(path: &Path, output_dir: &Path) -> String {
    path.strip_prefix(output_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn markdown_cell(text: &str) -> String {
    markdown_text(text).replace('|', "\\|")
}

fn markdown_text(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

#[derive(Debug)]
struct CacheTableIndexes {
    accesses: usize,
    local_l2: usize,
    local_cache: usize,
    external_cache: usize,
    local_dram: usize,
}

#[derive(Debug)]
struct CacheSummaryRow {
    table: String,
    item: String,
    accesses: f64,
    hits: f64,
    misses: f64,
    miss_rate: f64,
    local_l2: f64,
    local_cache: f64,
    external_cache: f64,
    local_dram: f64,
}

fn cache_table_indexes(
    table: &crate::benchmark::uprof::AmdUprofTable,
) -> Option<CacheTableIndexes> {
    Some(CacheTableIndexes {
        accesses: header_index(table, "L1_DC_ACCESSES_ALL.USER")?,
        local_l2: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_L2.USER")?,
        local_cache: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_CACHE.USER")?,
        external_cache: header_index(table, "L1_DEMAND_DC_REFILLS_EXTERNAL_CACHE_LOCAL.USER")?,
        local_dram: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_DRAM.USER")?,
    })
}

fn header_index(table: &crate::benchmark::uprof::AmdUprofTable, header: &str) -> Option<usize> {
    table
        .headers
        .iter()
        .position(|candidate| candidate == header)
}

fn parse_metric(value: &str) -> Option<f64> {
    value.parse().ok()
}

fn format_metric(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.4}")
    }
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

    mimalloc_stats: Vec<MimallocStats>,
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

            mimalloc_stats: Vec::new(),
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

        if let Some(stats) = r.mimalloc_stats.clone() {
            self.mimalloc_stats.push(stats);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_uses_runs_per_save_in_scenario() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let results = vec![
            BenchmarkRun {
                save_name: "alpha".to_string(),
                platform: "linux-x86_64".to_string(),
                factorio_version: "2.0".to_string(),
                ticks: 6000,
                index: 0,
                execution_time_ms: 100.0,
                avg_ms: 10.0,
                min_ms: 9.0,
                max_ms: 11.0,
                effective_ups: 60000.0,
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "alpha".to_string(),
                platform: "linux-x86_64".to_string(),
                factorio_version: "2.0".to_string(),
                ticks: 6000,
                index: 1,
                execution_time_ms: 110.0,
                avg_ms: 11.0,
                min_ms: 10.0,
                max_ms: 12.0,
                effective_ups: 54545.0,
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "beta".to_string(),
                platform: "linux-x86_64".to_string(),
                factorio_version: "2.0".to_string(),
                ticks: 6000,
                index: 0,
                execution_time_ms: 120.0,
                avg_ms: 12.0,
                min_ms: 11.0,
                max_ms: 13.0,
                effective_ups: 50000.0,
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "beta".to_string(),
                platform: "linux-x86_64".to_string(),
                factorio_version: "2.0".to_string(),
                ticks: 6000,
                index: 1,
                execution_time_ms: 130.0,
                avg_ms: 13.0,
                min_ms: 12.0,
                max_ms: 14.0,
                effective_ups: 46153.0,
                ..Default::default()
            },
        ];

        write_report(&results, None, path).expect("write report");

        let report = std::fs::read_to_string(path.join("results.md")).expect("read report");
        assert!(report.contains("Each save was tested for 6000 tick(s) and 2 run(s)"));
    }

    #[test]
    fn test_report_archives_and_renders_amd_uprof_report() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let source_dir = temp_dir.path().join("source-session");
        std::fs::create_dir_all(&source_dir).expect("source dir");
        let source_report = source_dir.join("report.csv");
        std::fs::write(
            &source_report,
            r#"AMD uProf (Version:5.3.518.0)
PERFORMANCE ANALYSIS REPORT

PROFILE DETAILS
Profile Session Type,Hotspots
Profile Duration,4.389 sec
Selected View,hotspots

APPLICATION PERFORMANCE SNAPSHOT
Thread Count,24

10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME)
FUNCTION,CPU_TIME,L1_DC_ACCESSES_ALL.USER,L1_DEMAND_DC_REFILLS_LOCAL_L2.USER,L1_DEMAND_DC_REFILLS_LOCAL_CACHE.USER,L1_DEMAND_DC_REFILLS_EXTERNAL_CACHE_LOCAL.USER,L1_DEMAND_DC_REFILLS_LOCAL_DRAM.USER,Module
foo,1.230,100.0000,10.0000,5.0000,0.0000,5.0000,libfoo.so
"#,
        )
        .expect("write source report");

        let results = vec![BenchmarkRun {
            save_name: "alpha".to_string(),
            platform: "linux-x86_64".to_string(),
            factorio_version: "2.0".to_string(),
            ticks: 6000,
            index: 0,
            execution_time_ms: 100.0,
            avg_ms: 10.0,
            min_ms: 9.0,
            max_ms: 11.0,
            effective_ups: 60000.0,
            amd_uprof: Some(crate::benchmark::uprof::AmdUprofRun {
                session_paths: vec![source_dir],
                reports: vec![crate::benchmark::uprof::AmdUprofReportArtifact::new(
                    source_report,
                )],
            }),
            ..Default::default()
        }];

        write_report(&results, None, path).expect("write report");

        let copied = path.join("uprof/alpha/run_0/report_0.csv");
        assert!(copied.exists(), "report.csv should be copied");

        let report = std::fs::read_to_string(path.join("results.md")).expect("read report");
        assert!(
            report.contains("## AMD uProf"),
            "report did not contain AMD section:\n{report}"
        );
        assert!(report.contains("Hotspots"));
        assert!(report.contains("10 HOTTEST FUNCTIONS"));
        assert!(report.contains("Estimated L1 Data Cache Summary"));
        assert!(report.contains("20.00%"));
        assert!(report.contains("foo"));
        assert!(report.contains("uprof/alpha/run_0/report_0.csv"));
    }
}
