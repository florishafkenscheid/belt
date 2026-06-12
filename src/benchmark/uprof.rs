use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::benchmark::parser::BenchmarkRun;

pub const MAX_TABLES_PER_REPORT: usize = 32;
pub const MAX_ROWS_PER_TABLE: usize = 100;
pub const MAX_CELLS_PER_ROW: usize = 64;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AmdUprofRun {
    pub session_paths: Vec<PathBuf>,
    pub reports: Vec<AmdUprofReportArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AmdUprofReportArtifact {
    pub original_path: PathBuf,
    pub copied_path: Option<PathBuf>,
    pub parsed: Option<AmdUprofParsedReport>,
    pub copy_error: Option<String>,
    pub parse_error: Option<String>,
}

impl AmdUprofReportArtifact {
    pub fn new(original_path: PathBuf) -> Self {
        Self {
            original_path,
            copied_path: None,
            parsed: None,
            copy_error: None,
            parse_error: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct AmdUprofParsedReport {
    pub metadata: Vec<AmdUprofMetadata>,
    pub tables: Vec<AmdUprofTable>,
    pub truncated: bool,
}

impl AmdUprofParsedReport {
    pub fn metadata_value(&self, key: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|entry| entry.key.eq_ignore_ascii_case(key))
            .map(|entry| entry.value.as_str())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AmdUprofMetadata {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AmdUprofTable {
    pub title: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

pub fn parse_report_csv(csv: &str) -> Result<AmdUprofParsedReport, csv::Error> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv.as_bytes());

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(
            record
                .iter()
                .map(|cell| cell.trim().to_string())
                .collect::<Vec<_>>(),
        );
    }

    Ok(parse_report_rows(&rows))
}

pub fn archive_and_parse_run(run: &mut BenchmarkRun, output_dir: &Path) {
    let Some(uprof) = run.amd_uprof.as_mut() else {
        return;
    };

    let artifact_dir = output_dir
        .join("uprof")
        .join(sanitize_path_component(&run.save_name))
        .join(format!("run_{}", run.index));

    if let Err(err) = fs::create_dir_all(&artifact_dir) {
        tracing::warn!(
            "Failed to create AMD uProf artifact directory {}: {err}",
            artifact_dir.display()
        );
    }

    for (index, report) in uprof.reports.iter_mut().enumerate() {
        let copied_path = artifact_dir.join(format!("report_{index}.csv"));
        let parse_path = if report.original_path.exists() {
            match fs::copy(&report.original_path, &copied_path) {
                Ok(_) => {
                    report.copied_path = Some(copied_path.clone());
                    copied_path
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to copy AMD uProf report {} to {}: {err}",
                        report.original_path.display(),
                        copied_path.display()
                    );
                    report.copy_error = Some(err.to_string());
                    report.original_path.clone()
                }
            }
        } else {
            let message = "report file does not exist".to_string();
            tracing::warn!(
                "Detected AMD uProf report path does not exist: {}",
                report.original_path.display()
            );
            report.copy_error = Some(message);
            report.original_path.clone()
        };

        match fs::read_to_string(&parse_path) {
            Ok(csv) => match parse_report_csv(&csv) {
                Ok(parsed) => report.parsed = Some(parsed),
                Err(err) => {
                    tracing::warn!(
                        "Failed to parse AMD uProf report {}: {err}",
                        parse_path.display()
                    );
                    report.parse_error = Some(err.to_string());
                }
            },
            Err(err) => {
                tracing::warn!(
                    "Failed to read AMD uProf report {}: {err}",
                    parse_path.display()
                );
                report.parse_error = Some(err.to_string());
            }
        }
    }
}

fn parse_report_rows(rows: &[Vec<String>]) -> AmdUprofParsedReport {
    let mut parsed = AmdUprofParsedReport::default();
    let mut index = 0;

    while index < rows.len() {
        let row = &rows[index];
        if is_blank(row) {
            index += 1;
            continue;
        }

        if let Some(title) = single_non_empty_cell(row) {
            let Some(next_index) = next_non_blank(rows, index + 1) else {
                break;
            };
            let next_row = &rows[next_index];

            if is_metadata_section(title) {
                index = parse_metadata_section(rows, next_index, &mut parsed);
                continue;
            }

            if non_empty_count(next_row) >= 2 {
                let (table, next_index) = parse_table(title, rows, next_index);
                if parsed.tables.len() < MAX_TABLES_PER_REPORT {
                    parsed.truncated |= table.truncated;
                    parsed.tables.push(table);
                } else {
                    parsed.truncated = true;
                }
                index = next_index;
                continue;
            }
        } else if non_empty_count(row) >= 2 {
            let key = normalize_metadata_key(&row[0]);
            let value = row[1].trim();
            if !key.is_empty() && !value.is_empty() {
                parsed.metadata.push(AmdUprofMetadata {
                    key,
                    value: value.to_string(),
                });
            }
        }

        index += 1;
    }

    parsed
}

fn parse_metadata_section(
    rows: &[Vec<String>],
    mut index: usize,
    parsed: &mut AmdUprofParsedReport,
) -> usize {
    while index < rows.len() {
        let row = &rows[index];
        if is_blank(row) {
            return index + 1;
        }
        if let Some(title) = single_non_empty_cell(row) {
            if title.ends_with(':') {
                index += 1;
                continue;
            }
            return index;
        }

        let key = row
            .first()
            .map(|cell| normalize_metadata_key(cell))
            .unwrap_or_default();
        let value = row.get(1).map(|cell| cell.trim()).unwrap_or_default();
        if !key.is_empty() && !value.is_empty() {
            parsed.metadata.push(AmdUprofMetadata {
                key,
                value: value.to_string(),
            });
        }
        index += 1;
    }

    index
}

fn parse_table(title: &str, rows: &[Vec<String>], header_index: usize) -> (AmdUprofTable, usize) {
    let mut headers = normalized_non_empty_cells(&rows[header_index]);
    if headers.len() > MAX_CELLS_PER_ROW {
        headers.truncate(MAX_CELLS_PER_ROW);
    }

    let mut table = AmdUprofTable {
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
        if single_non_empty_cell(row).is_some() {
            return (table, index);
        }

        if table.rows.len() < MAX_ROWS_PER_TABLE {
            let mut cells = normalized_cells(row);
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
        normalize_metadata_key(title).as_str(),
        "EXECUTION" | "PROFILE DETAILS" | "APPLICATION PERFORMANCE SNAPSHOT"
    )
}

fn normalize_metadata_key(key: &str) -> String {
    key.trim().trim_end_matches(':').to_string()
}

fn next_non_blank(rows: &[Vec<String>], start: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .skip(start)
        .find_map(|(index, row)| (!is_blank(row)).then_some(index))
}

fn normalized_cells(row: &[String]) -> Vec<String> {
    row.iter().map(|cell| cell.trim().to_string()).collect()
}

fn normalized_non_empty_cells(row: &[String]) -> Vec<String> {
    row.iter()
        .filter_map(|cell| {
            let cell = cell.trim();
            (!cell.is_empty()).then(|| cell.to_string())
        })
        .collect()
}

fn single_non_empty_cell(row: &[String]) -> Option<&str> {
    let mut cells = row.iter().filter(|cell| !cell.trim().is_empty());
    let first = cells.next()?;
    cells.next().is_none().then_some(first.trim())
}

fn non_empty_count(row: &[String]) -> usize {
    row.iter().filter(|cell| !cell.trim().is_empty()).count()
}

fn is_blank(row: &[String]) -> bool {
    non_empty_count(row) == 0
}

fn sanitize_path_component(component: &str) -> String {
    component
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotspots_report_csv() {
        let csv = r#"AMD uProf (Version:5.3.518.0)
PERFORMANCE ANALYSIS REPORT

EXECUTION
Target Path:,/home/blousy/.local/bin/factorio
Command Line Arguments:,--benchmark save.zip
Environment Variables:
CPU Details:,Family(0x1a)

PROFILE DETAILS
Profile Session Type:,Hotspots
Profile Duration:,4.389 sec
Selected View:,hotspots

APPLICATION PERFORMANCE SNAPSHOT
Thread Count,24

10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME)
FUNCTION,CPU_TIME,Module
foo,1.230,libfoo.so
bar,0.120,factorio
"#;

        let parsed = parse_report_csv(csv).expect("parse report");

        assert_eq!(
            parsed.metadata_value("Profile Session Type"),
            Some("Hotspots")
        );
        assert_eq!(parsed.metadata_value("CPU Details"), Some("Family(0x1a)"));
        assert_eq!(parsed.metadata_value("Thread Count"), Some("24"));
        assert_eq!(parsed.tables.len(), 1);
        assert_eq!(
            parsed.tables[0].title,
            "10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME)"
        );
        assert_eq!(parsed.tables[0].headers, ["FUNCTION", "CPU_TIME", "Module"]);
        assert_eq!(parsed.tables[0].rows[0], ["foo", "1.230", "libfoo.so"]);
    }
}
