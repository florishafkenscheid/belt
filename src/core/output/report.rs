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
        uprof,
    },
    core::{
        calculate_base_differences,
        error::{BenchmarkErrorKind, Result},
        output::{self, ResultWriter, WriteData, ensure_output_dir},
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
            } => write_report(data, *template_path, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }

    fn append(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Report {
                data,
                template_path,
            } => append_report(data, *template_path, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }
}

/// Write the results to a Handlebars file
fn write_report(results: &[BenchmarkRun], template_path: Option<&Path>, path: &Path) -> Result<()> {
    const TPL_STR: &str = "# Factorio Benchmark Results\n\n**Platform:** {{platform}}\n**Factorio Version:** {{factorio_version}}\n**Date:** {{date}}\n\n## Scenario\n* Each save was tested for {{ticks}} tick(s) and {{runs}} run(s)\n\n## Results\n| Metric            | Description                           |\n| ----------------- | ------------------------------------- |\n| **Mean UPS**      | Updates per second – higher is better |\n| **Mean Avg (ms)** | Average frame time – lower is better  |\n| **Mean Min (ms)** | Minimum frame time – lower is better  |\n| **Mean Max (ms)** | Maximum frame time – lower is better  |\n\n| Save | Avg (ms) | Min (ms) | Max (ms) | UPS | Execution Time (ms) | % Difference from base |\n|------|----------|----------|----------|-----|---------------------|------------------------|\n{{#each results}}\n| {{save_name}} | {{avg_ms}} | {{min_ms}} | {{max_ms}} | {{{avg_effective_ups}}} | {{total_execution_time_ms}} | {{percentage_improvement}} |\n{{/each}}\n\n{{#if results.0.mimalloc}}\n## Memory (mimalloc)\n\n### What these numbers mean (practical interpretation)\n| Field | What it roughly indicates |\n|------|----------------------------|\n| **Committed (peak)** | Highest amount of memory backed by the OS during the run (best \"memory footprint\" trend metric). |\n| **Reserved (peak)** | Highest virtual address space reserved by the allocator. **If Committed > Reserved, the application uses direct `mmap`/`VirtualAlloc` outside the allocator** (e.g., for memory-mapped files or custom pools). |\n| **Peak RSS** | Highest resident set size (what was actually in RAM). Large gaps between Committed and RSS indicate sparse memory usage (hugepages, memory-mapped files, or reserved-but-untouched arenas). |\n| **Commit Efficiency** | `(Peak RSS / Committed Peak)` as percentage. <10% = sparse allocation (mostly reserved, not touched); >80% = dense working set. |\n| **Committed/Reserved (current)** | What the allocator still held at process exit. Not automatically a leak—mimalloc retains arenas for reuse. **Trend this across multiple runs; growth between identical runs indicates leaks.** |\n| **Pages / Abandoned (current + status)** | \"Not all freed\" is **normal**—the allocator caches pages for reuse. Abandoned blocks indicate thread-local heap fragments from terminated threads. Flag only if these numbers grow across benchmark iterations. |\n| **Thread Churn** | `(Threads Peak - Current)`. Values >0 indicate short-lived worker threads spawned during initialization (explains Abandoned blocks). |\n| **Threads (peak)** | Peak allocator thread count observed. If Peak > Current, expect elevated Abandoned blocks. |\n| **mmaps** | Number of OS allocation calls. Low counts (<50) with high memory usage indicate efficient arena reuse. High counts indicate frequent allocation pressure or fragmentation. |\n| **purges / resets** | Memory returned to OS. Usually 0 in benchmarks—non-zero indicates aggressive memory trimming or constrained environments. |\n\n### Summary (end-of-run heap stats)\n| Save | Committed Peak | Peak RSS | Commit Efficiency | Reserved Peak | Committed Current | Reserved Current | Pages Current | Pages Status | Abandoned Current | Abandoned Status | Thread Churn | Threads Peak | mmaps | purges | resets |\n|------|----------------|----------|-------------------|---------------|-------------------|------------------|---------------|-------------|-------------------|------------------|--------------|-------------|-------|--------|--------|\n{{#each results}}\n{{#each mimalloc}}\n| {{../save_name}} | {{committed_peak}} | {{peak_rss}} | {{commit_efficiency}} | {{reserved_peak}} | {{committed_current}} | {{reserved_current}} | {{pages_current}} | {{pages_status}} | {{abandoned_current}} | {{abandoned_status}} | {{thread_churn}} | {{threads_peak}} | {{mmaps}} | {{purges}} | {{resets}} |\n{{/each}}\n{{/each}}\n\n{{/if}}\n{{#if amd_uprof.summary_rows}}\n## AMD uProf\n\n| Save | Run | Profile | View | Duration | Threads | Session | Report |\n|------|-----|---------|------|----------|---------|---------|--------|\n{{#each amd_uprof.summary_rows}}\n| {{{save}}} | {{run}} | {{{profile}}} | {{{view}}} | {{{duration}}} | {{{threads}}} | {{{session}}} | {{{report}}} |\n{{/each}}\n\n{{#each amd_uprof.reports}}\n### {{{title}}}\n\n{{#if copy_error}}\nReport archive warning: {{{copy_error}}}\n\n{{/if}}\n{{#if parse_error}}\nReport parse warning: {{{parse_error}}}. Full CSV: `{{{report_path}}}`\n\n{{/if}}\n{{#if metadata_rows}}\n| Field | Value |\n|-------|-------|\n{{#each metadata_rows}}\n| {{{field}}} | {{{value}}} |\n{{/each}}\n\n{{/if}}\n{{#if cache_rows}}\n#### Estimated L1 Data Cache Summary\n\nEstimated from `L1_DC_ACCESSES_ALL.USER` and demand refill source counters.\n\n| Table | Item | Accesses | Est Hits | Est Misses | Est Miss Rate | L2 Refills | Cache Refills | External Cache Refills | DRAM Refills |\n|-------|------|----------|----------|------------|---------------|------------|---------------|------------------------|--------------|\n{{#each cache_rows}}\n| {{{table}}} | {{{item}}} | {{{accesses}}} | {{{hits}}} | {{{misses}}} | {{{miss_rate}}} | {{{local_l2}}} | {{{local_cache}}} | {{{external_cache}}} | {{{local_dram}}} |\n{{/each}}\n\n{{/if}}\n{{#if ibs_load_rows}}\n#### IBS Load Cache Summary\n\nReported by AMD IBS load views such as `ibs_op_ld` and `ibs_op_ld_lat`.\n\n| Table | Item | Loads | L1 Hit Rate | L1 Miss Rate | L2 Hit Rate | Local Cache Hit Rate | Peer Cache Hit Rate | Remote Cache Hit Rate | DRAM Hit Rate | Avg L1 Miss Latency |\n|-------|------|-------|-------------|--------------|-------------|----------------------|---------------------|-----------------------|---------------|---------------------|\n{{#each ibs_load_rows}}\n| {{{table}}} | {{{item}}} | {{{loads}}} | {{{l1_hit_rate}}} | {{{l1_miss_rate}}} | {{{l2_hit_rate}}} | {{{local_cache_hit_rate}}} | {{{peer_cache_hit_rate}}} | {{{remote_cache_hit_rate}}} | {{{dram_hit_rate}}} | {{{l1_miss_latency}}} |\n{{/each}}\n\n{{/if}}\n{{#each tables}}\n#### {{{title}}}\n\n|{{#each headers}} {{{this}}} |{{/each}}\n|{{#each headers}}------|{{/each}}\n{{#each rows}}\n|{{#each this}} {{{this}}} |{{/each}}\n{{/each}}\n\n{{#if truncated}}\nThis AMD uProf table was truncated in Markdown. Full CSV: `{{{../report_path}}}`\n\n{{/if}}\n{{/each}}\n{{#if truncated}}\nThis AMD uProf report was truncated in Markdown. Full CSV: `{{{report_path}}}`\n\n{{/if}}\n{{/each}}\n{{/if}}\n## Conclusion";
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
    let amd_uprof = output::uprof::build_section(&report_results, path);

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
        "amd_uprof": amd_uprof,
    });

    let rendered = handlebars.render("benchmark", &data)?;

    std::fs::write(&results_path, rendered)?;

    tracing::info!("Report written to {}", results_path.display());
    Ok(())
}

fn append_report(
    results: &[BenchmarkRun],
    template_path: Option<&Path>,
    path: &Path,
) -> Result<()> {
    let results_csv = path.join("results.csv");

    if !results_csv.exists() {
        return write_report(results, template_path, path);
    }

    let mut combined = read_benchmark_runs_from_csv(&results_csv)?;
    combined.extend_from_slice(results);

    calculate_base_differences(&mut combined);

    write_report(results, template_path, path)
}

fn read_benchmark_runs_from_csv(csv_path: &Path) -> Result<Vec<BenchmarkRun>> {
    let mut reader = csv::Reader::from_path(csv_path)?;
    let mut runs = Vec::new();

    for record in reader.records() {
        let record = record?;

        runs.push(BenchmarkRun {
            save_name: record.get(0).unwrap_or_default().to_string(),
            index: record.get(1).unwrap_or("0").parse()?,
            execution_time_ms: record.get(2).unwrap_or("0").parse()?,
            avg_ms: record.get(3).unwrap_or("0").parse()?,
            min_ms: record.get(4).unwrap_or("0").parse()?,
            max_ms: record.get(5).unwrap_or("0").parse()?,
            effective_ups: record.get(6).unwrap_or("0").parse()?,
            base_diff: record.get(7).unwrap_or("0").parse()?,
            ticks: record.get(8).unwrap_or("0").parse()?,
            factorio_version: record.get(9).unwrap_or("unknown").to_string(),
            platform: record.get(10).unwrap_or("unknown").to_string(),
            ..Default::default()
        });
    }

    Ok(runs)
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

10 HOTTEST FUNCTIONS (Sort Event - IBS_LOAD)
FUNCTION,IBS_LOAD,IBS_LD_L1_DC_HIT_RATE_%,IBS_LD_L1_DC_MISS_RATE_%,IBS_LD_L2_HIT_RATE_%,IBS_LD_LOCAL_CACHE_HIT_RATE_%,IBS_LD_PEER_CACHE_HIT_RATE_%,IBS_LD_RMT_CACHE_HIT_RATE_%,IBS_LD_DRAM_HIT_RATE_%,IBS_LD_L1_DC_MISS_LAT_AVE,Module
foo,200.0000,80.0000,20.0000,10.0000,7.0000,1.0000,0.0000,2.0000,42.5000,libfoo.so
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
        assert!(report.contains("IBS Load Cache Summary"));
        assert!(report.contains("42.5000"));
        assert!(report.contains("foo"));
        assert!(report.contains("uprof/alpha/run_0/report_0.csv"));
    }
}
