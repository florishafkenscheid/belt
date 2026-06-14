use std::path::Path;

use serde::Serialize;

use crate::benchmark::{
    parser::BenchmarkRun,
    uprof::{AmdUprofParsedReport, AmdUprofReportArtifact, AmdUprofTable},
};

pub(crate) fn build_section(results: &[BenchmarkRun], output_dir: &Path) -> AmdUprofSection {
    let mut section = AmdUprofSection::default();

    for run in results {
        let Some(uprof) = &run.amd_uprof else {
            continue;
        };

        if uprof.reports.is_empty() {
            for session in &uprof.session_paths {
                section.summary_rows.push(AmdUprofSummaryRow {
                    save: markdown_cell(&run.save_name),
                    run: run.index,
                    profile: String::new(),
                    view: String::new(),
                    duration: String::new(),
                    threads: String::new(),
                    session: markdown_cell(&display_path(session, output_dir)),
                    report: markdown_cell(&format!(
                        "Run `AMDuProfCLI report -i {}`",
                        session.display()
                    )),
                });
            }
            continue;
        }

        for (report_index, report) in uprof.reports.iter().enumerate() {
            let parsed = report.parsed.as_ref();
            let session = uprof
                .session_paths
                .last()
                .map(|path| display_path(path, output_dir))
                .unwrap_or_default();
            let report_path = report_path_cell(report, output_dir);

            section.summary_rows.push(AmdUprofSummaryRow {
                save: markdown_cell(&run.save_name),
                run: run.index,
                profile: markdown_cell(metadata_or_empty(parsed, "Profile Session Type")),
                view: markdown_cell(metadata_or_empty(parsed, "Selected View")),
                duration: markdown_cell(metadata_or_empty(parsed, "Profile Duration")),
                threads: markdown_cell(metadata_or_empty(parsed, "Thread Count")),
                session: markdown_cell(&session),
                report: markdown_cell(&report_path),
            });

            section.reports.push(AmdUprofReportView {
                title: markdown_text(&format!(
                    "{} / run_{} / report_{}",
                    run.save_name, run.index, report_index
                )),
                report_path: markdown_text(&report_path),
                copy_error: report.copy_error.as_deref().map(markdown_text),
                parse_error: report.parse_error.as_deref().map(markdown_text),
                metadata_rows: parsed.map(metadata_rows).unwrap_or_default(),
                cache_rows: parsed.map(cache_rows).unwrap_or_default(),
                ibs_load_rows: parsed.map(ibs_load_rows).unwrap_or_default(),
                tables: parsed.map(table_views).unwrap_or_default(),
                truncated: parsed.is_some_and(|parsed| parsed.truncated),
            });
        }
    }

    section
}

fn metadata_rows(parsed: &AmdUprofParsedReport) -> Vec<MetadataRow> {
    [
        "Profile Session Type",
        "Selected View",
        "Profile Duration",
        "Thread Count",
        "Data Folder",
    ]
    .iter()
    .filter_map(|key| {
        parsed.metadata_value(key).map(|value| MetadataRow {
            field: markdown_cell(key),
            value: markdown_cell(value),
        })
    })
    .collect()
}

fn cache_rows(parsed: &AmdUprofParsedReport) -> Vec<CacheSummaryRow> {
    parsed
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
                    table: markdown_cell(&table.title),
                    item: markdown_cell(row.first().map(String::as_str).unwrap_or("")),
                    accesses: format_metric(accesses),
                    hits: format_metric(hits),
                    misses: format_metric(misses),
                    miss_rate: format!("{miss_rate:.2}%"),
                    local_l2: format_metric(local_l2),
                    local_cache: format_metric(local_cache),
                    external_cache: format_metric(external_cache),
                    local_dram: format_metric(local_dram),
                })
            })
        })
        .collect()
}

fn ibs_load_rows(parsed: &AmdUprofParsedReport) -> Vec<IbsLoadSummaryRow> {
    parsed
        .tables
        .iter()
        .filter_map(|table| ibs_load_table_indexes(table).map(|indexes| (table, indexes)))
        .flat_map(|(table, indexes)| {
            table.rows.iter().map(move |row| IbsLoadSummaryRow {
                table: markdown_cell(&table.title),
                item: markdown_cell(row.first().map(String::as_str).unwrap_or("")),
                loads: format_optional_metric(metric_at(row, indexes.loads)),
                l1_hit_rate: format_optional_percent(metric_at(row, indexes.l1_hit_rate)),
                l1_miss_rate: format_optional_percent(metric_at(row, indexes.l1_miss_rate)),
                l2_hit_rate: format_optional_percent(metric_at(row, indexes.l2_hit_rate)),
                local_cache_hit_rate: format_optional_percent(metric_at(
                    row,
                    indexes.local_cache_hit_rate,
                )),
                peer_cache_hit_rate: format_optional_percent(metric_at(
                    row,
                    indexes.peer_cache_hit_rate,
                )),
                remote_cache_hit_rate: format_optional_percent(metric_at(
                    row,
                    indexes.remote_cache_hit_rate,
                )),
                dram_hit_rate: format_optional_percent(metric_at(row, indexes.dram_hit_rate)),
                l1_miss_latency: format_optional_metric(metric_at(row, indexes.l1_miss_latency)),
            })
        })
        .collect()
}

fn table_views(parsed: &AmdUprofParsedReport) -> Vec<TableView> {
    parsed
        .tables
        .iter()
        .map(|table| TableView {
            title: markdown_text(&table.title),
            headers: table
                .headers
                .iter()
                .map(|header| markdown_cell(header))
                .collect(),
            rows: table_rows(table),
            truncated: table.truncated,
        })
        .collect()
}

fn table_rows(table: &AmdUprofTable) -> Vec<Vec<String>> {
    table
        .rows
        .iter()
        .map(|row| {
            (0..table.headers.len())
                .map(|index| markdown_cell(row.get(index).map(String::as_str).unwrap_or("")))
                .collect()
        })
        .collect()
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

#[derive(Debug, Serialize)]
struct CacheSummaryRow {
    table: String,
    item: String,
    accesses: String,
    hits: String,
    misses: String,
    miss_rate: String,
    local_l2: String,
    local_cache: String,
    external_cache: String,
    local_dram: String,
}

#[derive(Debug)]
struct IbsLoadTableIndexes {
    loads: Option<usize>,
    l1_hit_rate: Option<usize>,
    l1_miss_rate: Option<usize>,
    l2_hit_rate: Option<usize>,
    local_cache_hit_rate: Option<usize>,
    peer_cache_hit_rate: Option<usize>,
    remote_cache_hit_rate: Option<usize>,
    dram_hit_rate: Option<usize>,
    l1_miss_latency: Option<usize>,
}

#[derive(Debug, Serialize)]
struct IbsLoadSummaryRow {
    table: String,
    item: String,
    loads: String,
    l1_hit_rate: String,
    l1_miss_rate: String,
    l2_hit_rate: String,
    local_cache_hit_rate: String,
    peer_cache_hit_rate: String,
    remote_cache_hit_rate: String,
    dram_hit_rate: String,
    l1_miss_latency: String,
}

fn cache_table_indexes(table: &AmdUprofTable) -> Option<CacheTableIndexes> {
    Some(CacheTableIndexes {
        accesses: header_index(table, "L1_DC_ACCESSES_ALL.USER")?,
        local_l2: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_L2.USER")?,
        local_cache: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_CACHE.USER")?,
        external_cache: header_index(table, "L1_DEMAND_DC_REFILLS_EXTERNAL_CACHE_LOCAL.USER")?,
        local_dram: header_index(table, "L1_DEMAND_DC_REFILLS_LOCAL_DRAM.USER")?,
    })
}

fn ibs_load_table_indexes(table: &AmdUprofTable) -> Option<IbsLoadTableIndexes> {
    let indexes = IbsLoadTableIndexes {
        loads: header_index(table, "IBS_LOAD"),
        l1_hit_rate: header_index(table, "IBS_LD_L1_DC_HIT_RATE_%"),
        l1_miss_rate: header_index(table, "IBS_LD_L1_DC_MISS_RATE_%"),
        l2_hit_rate: header_index(table, "IBS_LD_L2_HIT_RATE_%"),
        local_cache_hit_rate: header_index(table, "IBS_LD_LOCAL_CACHE_HIT_RATE_%"),
        peer_cache_hit_rate: header_index(table, "IBS_LD_PEER_CACHE_HIT_RATE_%"),
        remote_cache_hit_rate: header_index(table, "IBS_LD_RMT_CACHE_HIT_RATE_%"),
        dram_hit_rate: header_index(table, "IBS_LD_DRAM_HIT_RATE_%"),
        l1_miss_latency: header_index(table, "IBS_LD_L1_DC_MISS_LAT_AVE"),
    };

    let has_load_cache_metric = indexes.l1_hit_rate.is_some()
        || indexes.l1_miss_rate.is_some()
        || indexes.l2_hit_rate.is_some()
        || indexes.local_cache_hit_rate.is_some()
        || indexes.peer_cache_hit_rate.is_some()
        || indexes.remote_cache_hit_rate.is_some()
        || indexes.dram_hit_rate.is_some()
        || indexes.l1_miss_latency.is_some();

    has_load_cache_metric.then_some(indexes)
}

fn header_index(table: &AmdUprofTable, header: &str) -> Option<usize> {
    table
        .headers
        .iter()
        .position(|candidate| candidate == header)
}

fn parse_metric(value: &str) -> Option<f64> {
    value.parse().ok()
}

fn metric_at(row: &[String], index: Option<usize>) -> Option<f64> {
    parse_metric(row.get(index?)?)
}

fn format_metric(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.4}")
    }
}

fn format_optional_metric(value: Option<f64>) -> String {
    value.map(format_metric).unwrap_or_default()
}

fn format_optional_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2}%"))
        .unwrap_or_default()
}

#[derive(Default, Serialize)]
pub(crate) struct AmdUprofSection {
    summary_rows: Vec<AmdUprofSummaryRow>,
    reports: Vec<AmdUprofReportView>,
}

#[derive(Serialize)]
struct AmdUprofSummaryRow {
    save: String,
    run: u32,
    profile: String,
    view: String,
    duration: String,
    threads: String,
    session: String,
    report: String,
}

#[derive(Serialize)]
struct AmdUprofReportView {
    title: String,
    report_path: String,
    copy_error: Option<String>,
    parse_error: Option<String>,
    metadata_rows: Vec<MetadataRow>,
    cache_rows: Vec<CacheSummaryRow>,
    ibs_load_rows: Vec<IbsLoadSummaryRow>,
    tables: Vec<TableView>,
    truncated: bool,
}

#[derive(Serialize)]
struct MetadataRow {
    field: String,
    value: String,
}

#[derive(Serialize)]
struct TableView {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    truncated: bool,
}
