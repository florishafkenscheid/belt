//! The benchmark report (`results.md` by default), rendered by Handlebars from the built-in
//! template or a user's `--template-path`, with the variables B.E.L.T. 4.x gives templates:
//!
//! - `platform`, `factorio_version`, `date`, `ticks`, `runs`
//! - `results`: one entry per save, sorted by name, with `save_name`, `avg_ms`, `min_ms`,
//!   `max_ms`, `avg_effective_ups` (the best bolded), `total_execution_time_ms`,
//!   `percentage_improvement` and `mimalloc` (one entry per run that printed heap statistics)
//! - `amd_uprof`: the AMD uProf section (see [`super::uprof`])

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use handlebars::Handlebars;
use serde_json::json;

use super::mimalloc::MimallocStats;
use super::output::RunRecord;
use super::uprof::{self, UprofRun};

/// The template file as the repository keeps it (with a final newline, for editors and hooks).
const BUILT_IN_FILE: &str = include_str!("../../templates/results.md.hbs");

/// Which template renders the report.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportTemplate {
    /// `templates/results.md.hbs`, rendered without the file's final newline, so the report
    /// ends right after `## Conclusion`.
    BuiltIn,
    /// A user's Handlebars file. `report.html.hbs` writes `report.html`; a file without the
    /// `.hbs` extension writes `results.md`. An `.html` output bolds with `<strong>`.
    File(PathBuf),
}

impl ReportTemplate {
    pub fn output_name(&self) -> String {
        match self {
            Self::File(path) if path.extension().is_some_and(|extension| extension == "hbs") => {
                path.file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default()
            }
            Self::BuiltIn | Self::File(_) => "results.md".into(),
        }
    }

    fn bold(&self, text: &str) -> String {
        if self.output_name().ends_with(".html") {
            format!("<strong>{text}</strong>")
        } else {
            format!("**{text}**")
        }
    }

    fn register(&self, handlebars: &mut Handlebars) -> Result<(), ReportError> {
        match self {
            Self::BuiltIn => {
                let built_in = BUILT_IN_FILE.strip_suffix('\n').unwrap_or(BUILT_IN_FILE);
                handlebars
                    .register_template_string("report", built_in)
                    .map_err(|error| ReportError::Template(error.to_string()))
            }
            Self::File(path) => handlebars
                .register_template_file("report", path)
                .map_err(|error| ReportError::Template(format!("{}: {error}", path.display()))),
        }
    }
}

/// What a run printed besides its timings.
#[derive(Debug, Clone, PartialEq)]
pub struct RunExtras {
    pub save_name: String,
    pub run_index: u32,
    pub mimalloc: Option<MimallocStats>,
    pub uprof: Option<UprofRun>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("report template {0}")]
    Template(String),
    #[error("rendering the report: {0}")]
    Render(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Renders the report from every row (`records`, in run order) and this command's runs'
/// extras, writes it into `dir`, and returns its path. uProf reports are copied into `dir`.
pub fn write(
    records: &[RunRecord],
    extras: &[RunExtras],
    template: &ReportTemplate,
    date: &str,
    dir: &Path,
) -> Result<PathBuf, ReportError> {
    let mut handlebars = Handlebars::new();
    template.register(&mut handlebars)?;
    let archived: Vec<uprof::ArchivedRun> = extras
        .iter()
        .filter_map(|run| {
            run.uprof
                .as_ref()
                .filter(|found| !found.is_empty())
                .map(|found| uprof::archive(&run.save_name, run.run_index, found, dir))
        })
        .collect();
    let data = variables(
        records,
        extras,
        template,
        date,
        uprof::section(&archived, dir),
    );
    let rendered = handlebars
        .render("report", &data)
        .map_err(|error| ReportError::Render(error.to_string()))?;
    let path = dir.join(template.output_name());
    std::fs::write(&path, rendered).map_err(|source| ReportError::Io {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

fn variables(
    records: &[RunRecord],
    extras: &[RunExtras],
    template: &ReportTemplate,
    date: &str,
    amd_uprof: uprof::Section,
) -> serde_json::Value {
    let mut by_save: BTreeMap<&str, Vec<&RunRecord>> = BTreeMap::new();
    for record in records {
        by_save.entry(&record.save_name).or_default().push(record);
    }
    let mean = |runs: &[&RunRecord], value: fn(&RunRecord) -> f64| {
        runs.iter().map(|run| value(run)).sum::<f64>() / runs.len() as f64
    };
    // Whole numbers are truncated, as in 4.x.
    let ups: BTreeMap<&str, u64> = by_save
        .iter()
        .map(|(save, runs)| (*save, mean(runs, |run| run.effective_ups) as u64))
        .collect();
    let best = ups.values().copied().max().unwrap_or(0);
    let results: Vec<serde_json::Value> = by_save
        .iter()
        .map(|(save, runs)| {
            let min = runs.iter().map(|run| run.min_ms).fold(f64::INFINITY, f64::min);
            let max = runs.iter().map(|run| run.max_ms).fold(f64::NEG_INFINITY, f64::max);
            let save_ups = ups[save];
            let mimalloc: Vec<&MimallocStats> =
                extras.iter().filter(|run| run.save_name == *save).filter_map(|run| run.mimalloc.as_ref()).collect();
            json!({
                "save_name": save,
                "avg_ms": format!("{:.3}", mean(runs, |run| run.avg_ms)),
                "min_ms": format!("{min:.3}"),
                "max_ms": format!("{max:.3}"),
                "avg_effective_ups": if save_ups == best { template.bold(&save_ups.to_string()) } else { save_ups.to_string() },
                "total_execution_time_ms": runs.iter().map(|run| run.execution_time_ms).sum::<f64>() as u64,
                "percentage_improvement": format!("{:.2}%", mean(runs, |run| run.percentage_improvement)),
                "mimalloc": mimalloc,
            })
        })
        .collect();
    let first = records.first();
    json!({
        "platform": first.map(|run| run.platform.as_str()),
        "factorio_version": first.map(|run| run.factorio_version.as_str()),
        "date": date,
        "ticks": first.map(|run| run.ticks).unwrap_or(0),
        "runs": by_save.values().next().map(Vec::len).unwrap_or(0),
        "results": results,
        "amd_uprof": amd_uprof,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(save: &str, run_index: u32, ups: f64) -> RunRecord {
        RunRecord {
            save_name: save.into(),
            run_index,
            execution_time_ms: 1000.0,
            avg_ms: 1.0,
            min_ms: 0.5,
            max_ms: 2.0,
            effective_ups: ups,
            percentage_improvement: 0.0,
            ticks: 6000,
            factorio_version: "2.1.25".into(),
            platform: "linux-x86_64".into(),
        }
    }

    fn render(records: &[RunRecord], extras: &[RunExtras], template: &ReportTemplate) -> String {
        let dir = tempfile::tempdir().unwrap();
        let path = write(records, extras, template, "2026-11-24", dir.path()).unwrap();
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn results_md_matches_the_contract_template() {
        let mut slow = record("slow", 0, 100.0);
        slow.avg_ms = 1.23456;
        let mut fast = record("fast", 0, 150.0);
        fast.percentage_improvement = 50.0;

        let report = render(&[slow, fast], &[], &ReportTemplate::BuiltIn);

        assert_eq!(
            report,
            "# Factorio Benchmark Results

**Platform:** linux-x86_64
**Factorio Version:** 2.1.25
**Date:** 2026-11-24

## Scenario
* Each save was tested for 6000 tick(s) and 1 run(s)

## Results
| Metric            | Description                           |
| ----------------- | ------------------------------------- |
| **Mean UPS**      | Updates per second – higher is better |
| **Mean Avg (ms)** | Average frame time – lower is better  |
| **Mean Min (ms)** | Minimum frame time – lower is better  |
| **Mean Max (ms)** | Maximum frame time – lower is better  |

| Save | Avg (ms) | Min (ms) | Max (ms) | UPS | Execution Time (ms) | % Difference from base |
|------|----------|----------|----------|-----|---------------------|------------------------|
| fast | 1.000 | 0.500 | 2.000 | **150** | 1000 | 50.00% |
| slow | 1.235 | 0.500 | 2.000 | 100 | 1000 | 0.00% |

## Conclusion"
        );
    }

    #[test]
    fn results_md_bolds_every_save_with_the_highest_ups() {
        let report = render(
            &[
                record("a", 0, 150.2),
                record("b", 0, 150.9),
                record("c", 0, 90.0),
            ],
            &[],
            &ReportTemplate::BuiltIn,
        );

        assert_eq!(
            report
                .lines()
                .filter(|line| line.contains("**150**"))
                .count(),
            2
        );
    }

    #[test]
    fn results_md_truncates_ups_and_execution_time_to_whole_numbers() {
        let mut run = record("a", 0, 99.99);
        run.execution_time_ms = 1999.9;

        let report = render(&[run], &[], &ReportTemplate::BuiltIn);

        assert!(report.contains("| **99** | 1999 |"), "{report}");
    }

    #[test]
    fn results_md_rounds_avg_min_max_to_three_decimals_and_difference_to_two() {
        let mut run = record("a", 0, 100.0);
        (
            run.avg_ms,
            run.min_ms,
            run.max_ms,
            run.percentage_improvement,
        ) = (0.12345, 0.0004, 9.9996, 12.345);

        let report = render(&[run], &[], &ReportTemplate::BuiltIn);

        assert!(
            report.contains("| a | 0.123 | 0.000 | 10.000 | **100** | 1000 | 12.35% |"),
            "{report}"
        );
    }

    #[test]
    fn results_md_takes_the_lowest_min_and_highest_max_over_runs() {
        let mut first = record("a", 0, 100.0);
        (first.min_ms, first.max_ms) = (0.2, 3.0);
        let mut second = record("a", 1, 100.0);
        (second.min_ms, second.max_ms) = (0.1, 2.0);

        let report = render(&[first, second], &[], &ReportTemplate::BuiltIn);

        assert!(report.contains("| a | 1.000 | 0.100 | 3.000 |"), "{report}");
    }

    #[test]
    fn a_custom_template_gets_the_4x_variables_and_names_the_output_after_itself() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir.path().join("summary.html.hbs");
        std::fs::write(
            &template,
            "{{platform}} {{runs}}{{#each results}} {{save_name}}={{{avg_effective_ups}}}{{/each}}",
        )
        .unwrap();
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();

        let path = write(
            &[record("a", 0, 120.0)],
            &[],
            &ReportTemplate::File(template),
            "2026-11-24",
            &out,
        )
        .unwrap();

        assert_eq!(
            (
                path.file_name().unwrap().to_str().unwrap(),
                std::fs::read_to_string(&path).unwrap().as_str()
            ),
            ("summary.html", "linux-x86_64 1 a=<strong>120</strong>")
        );
    }

    #[test]
    fn the_mimalloc_section_lists_each_runs_heap_stats() {
        let stats = crate::benchmark::mimalloc::parse(include_str!("mimalloc_sample.txt")).unwrap();
        let extras = RunExtras {
            save_name: "a".into(),
            run_index: 0,
            mimalloc: Some(stats),
            uprof: None,
        };

        let report = render(
            &[record("a", 0, 100.0)],
            &[extras],
            &ReportTemplate::BuiltIn,
        );

        assert!(
            report.contains("## Memory (mimalloc)")
                && report.contains("| a | 800.0 MiB | 400.0 MiB | 50.0% |"),
            "{report}"
        );
    }

    #[test]
    fn the_amd_uprof_section_shows_the_copied_report_and_its_cache_summaries() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("report.csv");
        std::fs::write(
            &source,
            "PROFILE DETAILS
Profile Session Type,Hotspots
Selected View,hotspots

10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME)
FUNCTION,CPU_TIME,L1_DC_ACCESSES_ALL.USER,L1_DEMAND_DC_REFILLS_LOCAL_L2.USER,L1_DEMAND_DC_REFILLS_LOCAL_CACHE.USER,L1_DEMAND_DC_REFILLS_EXTERNAL_CACHE_LOCAL.USER,L1_DEMAND_DC_REFILLS_LOCAL_DRAM.USER,Module
foo,1.230,100.0000,10.0000,5.0000,0.0000,5.0000,libfoo.so

10 HOTTEST FUNCTIONS (Sort Event - IBS_LOAD)
FUNCTION,IBS_LOAD,IBS_LD_L1_DC_HIT_RATE_%,IBS_LD_L1_DC_MISS_LAT_AVE,Module
foo,200.0000,80.0000,42.5000,libfoo.so
",
        )
        .unwrap();
        let uprof = UprofRun {
            session_paths: vec![dir.path().to_path_buf()],
            report_paths: vec![source],
        };
        let extras = RunExtras {
            save_name: "alpha".into(),
            run_index: 0,
            mimalloc: None,
            uprof: Some(uprof),
        };
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();

        let path = write(
            &[record("alpha", 0, 100.0)],
            &[extras],
            &ReportTemplate::BuiltIn,
            "2026-11-24",
            &out,
        )
        .unwrap();

        let report = std::fs::read_to_string(path).unwrap();
        let expected = [
            "| alpha | 0 | Hotspots | hotspots |",
            "| uprof/alpha/run_0/report_0.csv |",
            "| 10 HOTTEST FUNCTIONS (Sort Event - CPU_TIME) | foo | 100 | 80 | 20 | 20.00% | 10 | 5 | 0 | 5 |",
            "| 10 HOTTEST FUNCTIONS (Sort Event - IBS_LOAD) | foo | 200 | 80.00% |",
        ];
        assert!(
            expected.iter().all(|line| report.contains(line)),
            "{report}"
        );
    }
}
