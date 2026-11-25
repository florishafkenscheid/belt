//! AMD uProf reports, ported from B.E.L.T. 4.x. B.E.L.T. doesn't run uProf: a wrapper script
//! given as the Factorio path runs it and prints breadcrumbs, which a run's output then holds:
//!
//! ```text
//! Generated data files path: /path/to/session
//! Generated report file: /path/to/session/report.csv
//! ```
//!
//! Reports are copied to `<output>/uprof/<save>/run_<index>/report_<n>.csv`, parsed, and shown in
//! the report's "AMD uProf" section.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Tables, rows and cells beyond these are left out of the Markdown, which then says so.
const MAX_TABLES_PER_REPORT: usize = 32;
const MAX_ROWS_PER_TABLE: usize = 100;
const MAX_CELLS_PER_ROW: usize = 64;

const SESSION_PREFIX: &str = "Generated data files path:";
const REPORT_PREFIX: &str = "Generated report file:";

/// The uProf breadcrumbs of one run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UprofRun {
    pub session_paths: Vec<PathBuf>,
    pub report_paths: Vec<PathBuf>,
}

impl UprofRun {
    /// Takes in one line of Factorio's output; returns whether it was a breadcrumb.
    pub fn read(&mut self, line: &str) -> bool {
        let line = line.trim();
        if let Some(path) = line.strip_prefix(SESSION_PREFIX) {
            self.session_paths.push(PathBuf::from(path.trim()));
            return true;
        }
        if let Some(path) = line.strip_prefix(REPORT_PREFIX) {
            self.report_paths.push(PathBuf::from(path.trim()));
            return true;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.session_paths.is_empty() && self.report_paths.is_empty()
    }
}

/// One run's breadcrumbs with its reports copied and parsed.
#[derive(Debug, Clone)]
pub struct ArchivedRun {
    pub save_name: String,
    pub run_index: u32,
    pub session_paths: Vec<PathBuf>,
    pub reports: Vec<ReportArtifact>,
}

/// One uProf report: where it came from, its copy, and what could be read from it. A failed copy
/// or parse is kept as a message the report shows.
#[derive(Debug, Clone)]
pub struct ReportArtifact {
    pub original_path: PathBuf,
    pub copied_path: Option<PathBuf>,
    pub parsed: Option<ParsedReport>,
    pub copy_error: Option<String>,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedReport {
    pub metadata: Vec<(String, String)>,
    pub tables: Vec<Table>,
    pub truncated: bool,
}

impl ParsedReport {
    fn metadata_value(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

/// Copies a run's reports into `output_dir` and parses them. Problems are warned about and kept
/// on the artifact, so the report shows them too.
pub fn archive(save_name: &str, run_index: u32, run: &UprofRun, output_dir: &Path) -> ArchivedRun {
    let artifact_dir = output_dir
        .join("uprof")
        .join(path_component(save_name))
        .join(format!("run_{run_index}"));
    if let Err(error) = std::fs::create_dir_all(&artifact_dir) {
        tracing::warn!("cannot create {}: {error}", artifact_dir.display());
    }
    let reports = run
        .report_paths
        .iter()
        .enumerate()
        .map(|(index, original)| {
            archive_report(original, &artifact_dir.join(format!("report_{index}.csv")))
        })
        .collect();
    ArchivedRun {
        save_name: save_name.to_string(),
        run_index,
        session_paths: run.session_paths.clone(),
        reports,
    }
}

fn archive_report(original: &Path, copy: &Path) -> ReportArtifact {
    let mut artifact = ReportArtifact {
        original_path: original.to_path_buf(),
        copied_path: None,
        parsed: None,
        copy_error: None,
        parse_error: None,
    };
    let parse_from = if !original.exists() {
        tracing::warn!("the AMD uProf report {} does not exist", original.display());
        artifact.copy_error = Some("report file does not exist".into());
        original.to_path_buf()
    } else {
        match std::fs::copy(original, copy) {
            Ok(_) => {
                artifact.copied_path = Some(copy.to_path_buf());
                copy.to_path_buf()
            }
            Err(error) => {
                tracing::warn!(
                    "cannot copy the AMD uProf report {} to {}: {error}",
                    original.display(),
                    copy.display()
                );
                artifact.copy_error = Some(error.to_string());
                original.to_path_buf()
            }
        }
    };
    match std::fs::read_to_string(&parse_from)
        .map_err(|error| error.to_string())
        .and_then(|csv| parse_csv(&csv))
    {
        Ok(parsed) => artifact.parsed = Some(parsed),
        Err(error) => {
            tracing::warn!(
                "cannot read the AMD uProf report {}: {error}",
                parse_from.display()
            );
            artifact.parse_error = Some(error);
        }
    }
    artifact
}

/// A uProf report CSV: metadata sections (`EXECUTION`, `PROFILE DETAILS`, …) and titled tables.
pub fn parse_csv(csv: &str) -> Result<ParsedReport, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        rows.push(
            record
                .iter()
                .map(|cell| cell.trim().to_string())
                .collect::<Vec<_>>(),
        );
    }
    Ok(parse_rows(&rows))
}

fn parse_rows(rows: &[Vec<String>]) -> ParsedReport {
    let mut parsed = ParsedReport::default();
    let mut index = 0;
    while index < rows.len() {
        let row = &rows[index];
        if is_blank(row) {
            index += 1;
            continue;
        }
        if let Some(title) = single_cell(row) {
            let Some(next) = next_non_blank(rows, index + 1) else {
                break;
            };
            if is_metadata_section(title) {
                index = parse_metadata_section(rows, next, &mut parsed);
                continue;
            }
            if non_empty_count(&rows[next]) >= 2 {
                let (table, after) = parse_table(title, rows, next);
                if parsed.tables.len() < MAX_TABLES_PER_REPORT {
                    parsed.truncated |= table.truncated;
                    parsed.tables.push(table);
                } else {
                    parsed.truncated = true;
                }
                index = after;
                continue;
            }
        } else if non_empty_count(row) >= 2 {
            push_metadata(&mut parsed, &row[0], &row[1]);
        }
        index += 1;
    }
    parsed
}

fn push_metadata(parsed: &mut ParsedReport, key: &str, value: &str) {
    let key = metadata_key(key);
    let value = value.trim();
    if !key.is_empty() && !value.is_empty() {
        parsed.metadata.push((key, value.to_string()));
    }
}

fn parse_metadata_section(
    rows: &[Vec<String>],
    mut index: usize,
    parsed: &mut ParsedReport,
) -> usize {
    while index < rows.len() {
        let row = &rows[index];
        if is_blank(row) {
            return index + 1;
        }
        if let Some(title) = single_cell(row) {
            if title.ends_with(':') {
                index += 1;
                continue;
            }
            return index;
        }
        let key = row.first().map(String::as_str).unwrap_or_default();
        let value = row.get(1).map(String::as_str).unwrap_or_default();
        push_metadata(parsed, key, value);
        index += 1;
    }
    index
}

fn parse_table(title: &str, rows: &[Vec<String>], header_index: usize) -> (Table, usize) {
    let mut headers: Vec<String> = rows[header_index]
        .iter()
        .filter(|cell| !cell.trim().is_empty())
        .map(|cell| cell.trim().to_string())
        .collect();
    headers.truncate(MAX_CELLS_PER_ROW);
    let mut table = Table {
        title: title.to_string(),
        headers,
        rows: Vec::new(),
        truncated: false,
    };
    let mut index = header_index + 1;
    while index < rows.len() {
        let row = &rows[index];
        if is_blank(row) {
            return (table, index + 1);
        }
        if single_cell(row).is_some() {
            return (table, index);
        }
        if table.rows.len() < MAX_ROWS_PER_TABLE {
            let mut cells: Vec<String> = row.iter().map(|cell| cell.trim().to_string()).collect();
            if cells.len() > MAX_CELLS_PER_ROW {
                cells.truncate(MAX_CELLS_PER_ROW);
                table.truncated = true;
            }
            table.rows.push(cells);
        } else {
            table.truncated = true;
        }
        index += 1;
    }
    (table, index)
}

fn is_metadata_section(title: &str) -> bool {
    matches!(
        metadata_key(title).as_str(),
        "EXECUTION" | "PROFILE DETAILS" | "APPLICATION PERFORMANCE SNAPSHOT"
    )
}

fn metadata_key(key: &str) -> String {
    key.trim().trim_end_matches(':').to_string()
}

fn next_non_blank(rows: &[Vec<String>], start: usize) -> Option<usize> {
    (start..rows.len()).find(|&index| !is_blank(&rows[index]))
}

/// The row's only non-empty cell, when it has exactly one.
fn single_cell(row: &[String]) -> Option<&str> {
    let mut cells = row.iter().filter(|cell| !cell.trim().is_empty());
    let first = cells.next()?;
    if cells.next().is_some() {
        return None;
    }
    Some(first.trim())
}

fn non_empty_count(row: &[String]) -> usize {
    row.iter().filter(|cell| !cell.trim().is_empty()).count()
}

fn is_blank(row: &[String]) -> bool {
    non_empty_count(row) == 0
}

fn path_component(component: &str) -> String {
    component
        .chars()
        .map(|character| {
            if matches!(character, '/' | '\\' | ':') {
                '_'
            } else {
                character
            }
        })
        .collect()
}

/// The report's `amd_uprof` variable: a summary row per report (or per session without one)
/// and a view per report.
#[derive(Debug, Default, Serialize)]
pub struct Section {
    summary_rows: Vec<SummaryRow>,
    reports: Vec<ReportView>,
}

#[derive(Debug, Serialize)]
struct SummaryRow {
    save: String,
    run: u32,
    profile: String,
    view: String,
    duration: String,
    threads: String,
    session: String,
    report: String,
}

#[derive(Debug, Serialize)]
struct ReportView {
    title: String,
    report_path: String,
    copy_error: Option<String>,
    parse_error: Option<String>,
    metadata_rows: Vec<MetadataRow>,
    cache_rows: Vec<CacheRow>,
    ibs_load_rows: Vec<IbsLoadRow>,
    tables: Vec<TableView>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct MetadataRow {
    field: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct TableView {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct CacheRow {
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

#[derive(Debug, Serialize)]
struct IbsLoadRow {
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

/// The report section for every archived run, with paths shown relative to `output_dir`.
pub fn section(runs: &[ArchivedRun], output_dir: &Path) -> Section {
    let mut section = Section::default();
    for run in runs {
        if run.reports.is_empty() {
            for session in &run.session_paths {
                section.summary_rows.push(SummaryRow {
                    save: cell(&run.save_name),
                    run: run.run_index,
                    profile: String::new(),
                    view: String::new(),
                    duration: String::new(),
                    threads: String::new(),
                    session: cell(&display_path(session, output_dir)),
                    report: cell(&format!(
                        "Run `AMDuProfCLI report -i {}`",
                        session.display()
                    )),
                });
            }
            continue;
        }
        for (report_index, report) in run.reports.iter().enumerate() {
            let parsed = report.parsed.as_ref();
            let metadata = |key: &str| {
                cell(
                    parsed
                        .and_then(|parsed| parsed.metadata_value(key))
                        .unwrap_or_default(),
                )
            };
            let session = run
                .session_paths
                .last()
                .map(|path| display_path(path, output_dir))
                .unwrap_or_default();
            let report_path = display_path(
                report.copied_path.as_ref().unwrap_or(&report.original_path),
                output_dir,
            );
            section.summary_rows.push(SummaryRow {
                save: cell(&run.save_name),
                run: run.run_index,
                profile: metadata("Profile Session Type"),
                view: metadata("Selected View"),
                duration: metadata("Profile Duration"),
                threads: metadata("Thread Count"),
                session: cell(&session),
                report: cell(&report_path),
            });
            section.reports.push(ReportView {
                title: text(&format!(
                    "{} / run_{} / report_{report_index}",
                    run.save_name, run.run_index
                )),
                report_path: text(&report_path),
                copy_error: report.copy_error.as_deref().map(text),
                parse_error: report.parse_error.as_deref().map(text),
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

fn metadata_rows(parsed: &ParsedReport) -> Vec<MetadataRow> {
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
            field: cell(key),
            value: cell(value),
        })
    })
    .collect()
}

fn header_index(table: &Table, header: &str) -> Option<usize> {
    table
        .headers
        .iter()
        .position(|candidate| candidate == header)
}

fn metric_at(row: &[String], index: Option<usize>) -> Option<f64> {
    row.get(index?)?.parse().ok()
}

/// A whole number without decimals, otherwise four.
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

/// The L1 data cache estimate, for tables with the access and demand refill counters.
fn cache_rows(parsed: &ParsedReport) -> Vec<CacheRow> {
    let mut rows = Vec::new();
    for table in &parsed.tables {
        let columns = [
            "L1_DC_ACCESSES_ALL.USER",
            "L1_DEMAND_DC_REFILLS_LOCAL_L2.USER",
            "L1_DEMAND_DC_REFILLS_LOCAL_CACHE.USER",
            "L1_DEMAND_DC_REFILLS_EXTERNAL_CACHE_LOCAL.USER",
            "L1_DEMAND_DC_REFILLS_LOCAL_DRAM.USER",
        ]
        .map(|header| header_index(table, header));
        if columns.iter().any(Option::is_none) {
            continue;
        }
        for row in &table.rows {
            let values: Option<Vec<f64>> = columns
                .iter()
                .map(|&column| metric_at(row, column))
                .collect();
            let Some([accesses, local_l2, local_cache, external_cache, local_dram]) =
                values.and_then(|values| <[f64; 5]>::try_from(values).ok())
            else {
                continue;
            };
            let misses = local_l2 + local_cache + external_cache + local_dram;
            let miss_rate = if accesses > 0.0 {
                misses / accesses * 100.0
            } else {
                0.0
            };
            rows.push(CacheRow {
                table: cell(&table.title),
                item: cell(row.first().map(String::as_str).unwrap_or_default()),
                accesses: format_metric(accesses),
                hits: format_metric((accesses - misses).max(0.0)),
                misses: format_metric(misses),
                miss_rate: format!("{miss_rate:.2}%"),
                local_l2: format_metric(local_l2),
                local_cache: format_metric(local_cache),
                external_cache: format_metric(external_cache),
                local_dram: format_metric(local_dram),
            });
        }
    }
    rows
}

/// The IBS load summary, for tables with at least one load cache metric.
fn ibs_load_rows(parsed: &ParsedReport) -> Vec<IbsLoadRow> {
    let mut rows = Vec::new();
    for table in &parsed.tables {
        let column = |header: &str| header_index(table, header);
        let rates = [
            column("IBS_LD_L1_DC_HIT_RATE_%"),
            column("IBS_LD_L1_DC_MISS_RATE_%"),
            column("IBS_LD_L2_HIT_RATE_%"),
            column("IBS_LD_LOCAL_CACHE_HIT_RATE_%"),
            column("IBS_LD_PEER_CACHE_HIT_RATE_%"),
            column("IBS_LD_RMT_CACHE_HIT_RATE_%"),
            column("IBS_LD_DRAM_HIT_RATE_%"),
        ];
        let latency = column("IBS_LD_L1_DC_MISS_LAT_AVE");
        if rates.iter().all(Option::is_none) && latency.is_none() {
            continue;
        }
        let loads = column("IBS_LOAD");
        for row in &table.rows {
            let rate = |index: usize| format_optional_percent(metric_at(row, rates[index]));
            rows.push(IbsLoadRow {
                table: cell(&table.title),
                item: cell(row.first().map(String::as_str).unwrap_or_default()),
                loads: format_optional_metric(metric_at(row, loads)),
                l1_hit_rate: rate(0),
                l1_miss_rate: rate(1),
                l2_hit_rate: rate(2),
                local_cache_hit_rate: rate(3),
                peer_cache_hit_rate: rate(4),
                remote_cache_hit_rate: rate(5),
                dram_hit_rate: rate(6),
                l1_miss_latency: format_optional_metric(metric_at(row, latency)),
            });
        }
    }
    rows
}

fn table_views(parsed: &ParsedReport) -> Vec<TableView> {
    parsed
        .tables
        .iter()
        .map(|table| TableView {
            title: text(&table.title),
            headers: table.headers.iter().map(|header| cell(header)).collect(),
            rows: table
                .rows
                .iter()
                .map(|row| {
                    (0..table.headers.len())
                        .map(|index| cell(row.get(index).map(String::as_str).unwrap_or_default()))
                        .collect()
                })
                .collect(),
            truncated: table.truncated,
        })
        .collect()
}

fn display_path(path: &Path, output_dir: &Path) -> String {
    path.strip_prefix(output_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Text for a Markdown table cell: one line, pipes escaped.
fn cell(value: &str) -> String {
    text(value).replace('|', "\\|")
}

/// Text on one Markdown line.
fn text(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_name_the_session_and_report() {
        let mut run = UprofRun::default();
        for line in [
            "Performed 10 updates in 100.000 ms",
            "Generated data files path: /tmp/s",
            "Generated report file: /tmp/s/report.csv",
        ] {
            run.read(line);
        }

        assert_eq!(
            run,
            UprofRun {
                session_paths: vec![PathBuf::from("/tmp/s")],
                report_paths: vec![PathBuf::from("/tmp/s/report.csv")]
            }
        );
    }

    #[test]
    fn a_hotspots_report_yields_its_metadata_and_tables() {
        let csv = "AMD uProf (Version:5.3.518.0)
PERFORMANCE ANALYSIS REPORT

EXECUTION
Target Path:,/home/user/.local/bin/factorio
Environment Variables:
CPU Details:,Family(0x1a)

PROFILE DETAILS
Profile Session Type:,Hotspots
Profile Duration:,4.389 sec

APPLICATION PERFORMANCE SNAPSHOT
Thread Count,24

10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME)
FUNCTION,CPU_TIME,Module
foo,1.230,libfoo.so
bar,0.120,factorio
";

        let parsed = parse_csv(csv).unwrap();

        let read = (
            parsed.metadata_value("Profile Session Type"),
            parsed.metadata_value("Thread Count"),
            parsed.tables[0].rows[0].clone(),
        );
        assert_eq!(
            read,
            (
                Some("Hotspots"),
                Some("24"),
                vec!["foo".to_string(), "1.230".into(), "libfoo.so".into()]
            )
        );
    }

    #[test]
    fn a_report_is_copied_to_its_runs_uprof_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report.csv");
        std::fs::write(&source, "PROFILE DETAILS\nProfile Session Type,Hotspots\n").unwrap();
        let run = UprofRun {
            session_paths: vec![],
            report_paths: vec![source],
        };
        let output = dir.path().join("out");

        let archived = archive("alpha", 2, &run, &output);

        assert_eq!(
            archived.reports[0].copied_path,
            Some(output.join("uprof/alpha/run_2/report_0.csv"))
        );
    }
}
