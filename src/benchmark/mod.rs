//! `benchmark`: measure how fast Factorio simulates saves, and write the contract files.

pub mod mimalloc;
pub mod output;
pub mod parse;
pub mod report;
pub mod schedule;
pub mod uprof;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::BenchmarkSettings;
use crate::failure::{FailureReason, Failures, RunFailure};
use crate::saves::Save;
use crate::terminal::Progress;
use output::{
    Compression, CpuRow, Existing, OutputError, RunRecord, Selection, VerboseFiles, VerboseTable,
};
use parse::{Measurement, VerboseTimings};

/// Runs one benchmark of one save. Implemented by the Factorio adapter.
pub trait BenchmarkRunner {
    /// Readies everything a run of `save` needs (its mods, synced if necessary), so that the
    /// timed runs and their CPU samples cover only Factorio's benchmark.
    fn prepare(&mut self, save: &Save) -> Result<(), RunFailure>;

    fn benchmark(
        &mut self,
        save: &Save,
        request: &BenchmarkRequest,
    ) -> Result<Measurement, RunFailure>;
}

/// What to ask Factorio for in one run.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkRequest {
    pub ticks: u32,
    /// Whether per-tick verbose timings are wanted. Every column Factorio has is asked for, and
    /// the benchmark picks the requested ones from what comes back.
    pub verbose: bool,
}

/// Samples CPU core frequencies while a run executes.
pub trait CpuMonitor {
    fn start(&self) -> Box<dyn CpuRecording>;
}

pub trait CpuRecording {
    fn stop(self: Box<Self>) -> Vec<CpuSample>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct CpuSample {
    pub core_index: usize,
    pub mhz: u64,
    pub ms_since_start: u64,
}

/// The monitor when CPU frequency recording is off: records nothing.
pub struct NoCpuMonitor;

impl CpuMonitor for NoCpuMonitor {
    fn start(&self) -> Box<dyn CpuRecording> {
        Box::new(NoCpuMonitor)
    }
}

impl CpuRecording for NoCpuMonitor {
    fn stop(self: Box<Self>) -> Vec<CpuSample> {
        Vec::new()
    }
}

/// The metric that also defines `max_ms` when verbose timings are on.
const WHOLE_UPDATE: &str = "wholeUpdate";
/// Factorio columns that are never metrics.
const NOT_METRICS: &[&str] = &["timestamp"];

/// Which verbose metrics the user asked for.
#[derive(Debug, Clone, PartialEq)]
enum Verbose {
    Off,
    All,
    Metrics(Vec<String>),
}

impl Verbose {
    /// Factorio's own `tick` and `timestamp` columns are never metrics, so asking for them is
    /// refused before any run.
    fn from_settings(metrics: &[String]) -> Result<Self, BenchmarkError> {
        let not_metrics: Vec<String> = metrics
            .iter()
            .filter(|metric| metric.as_str() == "tick" || NOT_METRICS.contains(&metric.as_str()))
            .cloned()
            .collect();
        if !not_metrics.is_empty() {
            return Err(BenchmarkError::NotMetrics(not_metrics));
        }
        Ok(match metrics {
            [] => Self::Off,
            [all] if all == "all" => Self::All,
            metrics => Self::Metrics(metrics.to_vec()),
        })
    }

    fn wanted(&self) -> bool {
        *self != Self::Off
    }

    /// `all` keeps every Factorio column, its empty last one included; a list keeps only itself.
    fn selection(&self) -> Selection {
        if *self == Self::All {
            Selection::All
        } else {
            Selection::Requested
        }
    }

    /// The indexes of the columns to write, in Factorio's order. Requested names Factorio
    /// didn't report are an error.
    fn columns(&self, timings: &VerboseTimings) -> Result<Vec<usize>, Vec<String>> {
        let is_metric = |column: &String| !NOT_METRICS.contains(&column.as_str());
        match self {
            Self::Off => Ok(Vec::new()),
            Self::All => Ok((0..timings.columns.len())
                .filter(|&index| is_metric(&timings.columns[index]))
                .collect()),
            Self::Metrics(metrics) => {
                let unknown: Vec<String> = metrics
                    .iter()
                    .filter(|metric| !timings.columns.contains(metric))
                    .cloned()
                    .collect();
                if !unknown.is_empty() {
                    return Err(unknown);
                }
                Ok((0..timings.columns.len())
                    .filter(|&index| metrics.contains(&timings.columns[index]))
                    .collect())
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error(transparent)]
    Output(#[from] OutputError),
    #[error(transparent)]
    Report(#[from] report::ReportError),
    #[error("Factorio doesn't know the verbose metric(s) {}", .0.join(", "))]
    UnknownMetrics(Vec<String>),
    #[error("{} is not a verbose metric (Factorio's own column, never written)", .0.join(", "))]
    NotMetrics(Vec<String>),
    #[error("cannot create output directory {path}: {source}")]
    OutputDir {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

/// How a benchmark command ended, short of an error that stopped it.
#[derive(Debug, Default)]
pub struct Outcome {
    pub failures: Failures,
    pub interrupted: bool,
}

/// The output files of one benchmark command: checked before the first run, extended after
/// each run (verbose metrics, CPU frequencies), and finished with `results.csv` and `results.md`.
struct BenchmarkFiles {
    dir: PathBuf,
    verbose: Verbose,
    verbose_files: VerboseFiles,
    /// Whether `cpu_freq.csv` already holds this command's (or, appending, earlier) rows.
    cpu_started: bool,
    /// Rows from an earlier command (when appending), then this command's, in run order.
    records: Vec<RunRecord>,
    /// What this command's runs printed besides timings (mimalloc, uProf).
    extras: Vec<report::RunExtras>,
    template: report::ReportTemplate,
}

impl BenchmarkFiles {
    /// Creates the output directory and, before any run, refuses existing files this command
    /// can't extend: CSV headers of another format, verbose files of the other compression, or a
    /// truncated compressed file.
    fn open(settings: &BenchmarkSettings, save_names: &[String]) -> Result<Self, BenchmarkError> {
        let dir = settings.output.clone();
        std::fs::create_dir_all(&dir).map_err(|source| BenchmarkError::OutputDir {
            path: dir.clone(),
            source,
        })?;
        let verbose = Verbose::from_settings(&settings.verbose_metrics)?;
        let compression = if settings.compress {
            Compression::Zstd
        } else {
            Compression::Plain
        };
        let existing = if settings.append {
            Existing::Append
        } else {
            Existing::Replace
        };
        let verbose_files = VerboseFiles::new(&dir, compression, existing);
        let records = if settings.append {
            output::check_header(&dir.join(output::CPU_FILE), output::CPU_HEADER)?;
            output::read_results(&dir.join(output::RESULTS_FILE))?
        } else {
            Vec::new()
        };
        if verbose != Verbose::Off {
            verbose_files.check(save_names)?;
        }
        let template = match &settings.template_path {
            Some(path) => report::ReportTemplate::File(path.clone()),
            None => report::ReportTemplate::BuiltIn,
        };
        Ok(Self {
            dir,
            verbose,
            verbose_files,
            cpu_started: settings.append,
            records,
            extras: Vec::new(),
            template,
        })
    }

    /// The run index of a save's first run in this command: after its existing rows.
    fn first_index(&self, save_name: &str) -> u32 {
        self.records
            .iter()
            .filter(|record| record.save_name == save_name)
            .map(|record| record.run_index + 1)
            .max()
            .unwrap_or(0)
    }

    /// Writes one run's verbose metrics and CPU samples, and keeps its `results.csv` row.
    /// With verbose timings, `max_ms` is the highest `wholeUpdate` after the first tick.
    fn record(
        &mut self,
        save_name: &str,
        run_index: u32,
        measurement: Measurement,
        samples: Vec<CpuSample>,
    ) -> Result<(), BenchmarkError> {
        let mut max_ms = measurement.max_ms;
        if let Some(timings) = &measurement.verbose {
            let columns = self
                .verbose
                .columns(timings)
                .map_err(BenchmarkError::UnknownMetrics)?;
            if let Some(whole) = whole_update_max_ms(timings) {
                max_ms = whole;
            }
            self.verbose_files.write_run(
                save_name,
                run_index,
                &table(timings, &columns, self.verbose.selection()),
            )?;
        }
        if !samples.is_empty() {
            let rows: Vec<CpuRow> = samples
                .into_iter()
                .map(|sample| CpuRow {
                    save_name: save_name.to_string(),
                    run_index,
                    core_index: sample.core_index,
                    mhz: sample.mhz,
                    ms_since_start: sample.ms_since_start,
                })
                .collect();
            let existing = if self.cpu_started {
                Existing::Append
            } else {
                Existing::Replace
            };
            output::append_cpu_rows(&self.dir.join(output::CPU_FILE), &rows, existing)?;
            self.cpu_started = true;
        }
        self.extras.push(report::RunExtras {
            save_name: save_name.to_string(),
            run_index,
            mimalloc: measurement.mimalloc.clone(),
            uprof: measurement.uprof.clone(),
        });
        self.records.push(RunRecord {
            save_name: save_name.to_string(),
            run_index,
            execution_time_ms: measurement.execution_time_ms,
            avg_ms: measurement.avg_ms,
            min_ms: measurement.min_ms,
            max_ms,
            effective_ups: output::effective_ups(measurement.ticks, measurement.execution_time_ms),
            percentage_improvement: 0.0,
            ticks: measurement.ticks,
            factorio_version: measurement.factorio_version.clone(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        });
        Ok(())
    }

    /// Writes `results.csv` and the report from every row, when there is any.
    fn finish(mut self, date: &str) -> Result<(), BenchmarkError> {
        if self.records.is_empty() {
            return Ok(());
        }
        output::set_differences(&mut self.records);
        output::write_results(
            &self.dir.join(output::RESULTS_FILE),
            &output::in_csv_order(self.records.clone()),
        )?;
        report::write(&self.records, &self.extras, &self.template, date, &self.dir)?;
        Ok(())
    }
}

/// The strip-prefix rule: a save is reported by its file stem, minus `strip_prefix` when the
/// stem starts with it.
fn reported_name(save: &Save, strip_prefix: Option<&str>) -> String {
    let stripped = strip_prefix.and_then(|prefix| save.name.strip_prefix(prefix));
    stripped.unwrap_or(&save.name).to_string()
}

fn reported_names(saves: &[Save], strip_prefix: Option<&str>) -> Vec<String> {
    saves
        .iter()
        .map(|save| reported_name(save, strip_prefix))
        .collect()
}

/// One run about to execute: the save, its reported name and the run's index.
struct PlannedRun<'a> {
    save: &'a Save,
    name: &'a str,
    run_index: u32,
}

/// What became of one timed run.
enum RunEnd {
    /// Its rows are written (verbose metrics, CPU samples) or kept (`results.csv`).
    Recorded,
    Failed(RunFailure),
    /// Ctrl-C stopped it; no further run starts.
    Interrupted,
}

/// Readies every save's mods before any timed run. Returns the saves that can't run; their
/// failures, or an interrupt, go into `outcome`.
fn prepare_saves(
    saves: &[Save],
    names: &[String],
    runner: &mut dyn BenchmarkRunner,
    outcome: &mut Outcome,
) -> HashSet<usize> {
    let mut failed = HashSet::new();
    for (index, save) in saves.iter().enumerate() {
        match runner.prepare(save) {
            Ok(()) => {}
            Err(failure) if failure.reason == FailureReason::Interrupted => {
                outcome.interrupted = true;
                break;
            }
            Err(failure) => {
                outcome.failures.add(&names[index], failure);
                failed.insert(index);
            }
        }
    }
    failed
}

/// One timed run: CPU samples are taken around Factorio's benchmark, and a successful run's
/// rows go to the output files.
fn timed_run(
    run: &PlannedRun,
    runner: &mut dyn BenchmarkRunner,
    cpu: &dyn CpuMonitor,
    request: &BenchmarkRequest,
    files: &mut BenchmarkFiles,
) -> Result<RunEnd, BenchmarkError> {
    let recording = cpu.start();
    let result = runner.benchmark(run.save, request);
    let samples = recording.stop();
    match result {
        Ok(measurement) => {
            files.record(run.name, run.run_index, measurement, samples)?;
            Ok(RunEnd::Recorded)
        }
        Err(failure) if failure.reason == FailureReason::Interrupted => Ok(RunEnd::Interrupted),
        Err(failure) => Ok(RunEnd::Failed(failure)),
    }
}

pub fn run(
    settings: &BenchmarkSettings,
    saves: &[Save],
    runner: &mut dyn BenchmarkRunner,
    cpu: &dyn CpuMonitor,
    date: &str,
    rng: &mut fastrand::Rng,
) -> Result<Outcome, BenchmarkError> {
    let names = reported_names(saves, settings.strip_prefix.as_deref());
    let mut files = BenchmarkFiles::open(settings, &names)?;
    let first_indexes: Vec<u32> = names.iter().map(|name| files.first_index(name)).collect();
    let mut outcome = Outcome::default();
    let mut failed = prepare_saves(saves, &names, runner, &mut outcome);
    let request = BenchmarkRequest {
        ticks: settings.ticks,
        verbose: files.verbose.wanted(),
    };
    let plan = schedule::schedule(saves.len(), settings.runs, settings.run_order, rng);
    let progress = Progress::new(plan.len());
    for (position, scheduled) in plan.iter().enumerate() {
        if outcome.interrupted {
            break;
        }
        if failed.contains(&scheduled.save) {
            continue;
        }
        let run = PlannedRun {
            save: &saves[scheduled.save],
            name: &names[scheduled.save],
            run_index: first_indexes[scheduled.save] + scheduled.nth,
        };
        // Counted from 1, as 4.3.1 showed it; results.csv's run_index counts from 0.
        progress.start(
            position,
            &format!("{} (run {})", run.name, run.run_index + 1),
        );
        match timed_run(&run, runner, cpu, &request, &mut files)? {
            RunEnd::Recorded => {}
            RunEnd::Failed(failure) => {
                outcome.failures.add(run.name, failure);
                failed.insert(scheduled.save);
            }
            RunEnd::Interrupted => outcome.interrupted = true,
        }
    }
    if outcome.interrupted {
        progress.stop("Benchmarking interrupted.");
    } else {
        progress.finish("Benchmarking complete!");
    }
    files.finish(date)?;
    Ok(outcome)
}

/// The highest `wholeUpdate` of the run in ms, leaving out the first tick (a load spike).
fn whole_update_max_ms(timings: &VerboseTimings) -> Option<f64> {
    let column = timings
        .columns
        .iter()
        .position(|column| column == WHOLE_UPDATE)?;
    timings
        .rows
        .iter()
        .skip(1)
        .map(|row| row.values[column])
        .max()
        .map(|nanoseconds| nanoseconds as f64 / 1_000_000.0)
}

fn table(timings: &VerboseTimings, columns: &[usize], selection: Selection) -> VerboseTable {
    VerboseTable {
        selection,
        metrics: columns
            .iter()
            .map(|&index| timings.columns[index].clone())
            .collect(),
        rows: timings
            .rows
            .iter()
            .map(|row| {
                (
                    row.tick,
                    columns.iter().map(|&index| row.values[index]).collect(),
                )
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunOrder;
    use parse::VerboseRow;
    use std::path::{Path, PathBuf};

    /// Answers every run with the measurement for its save; a save named `broken` fails, and
    /// a save named `stop` is interrupted.
    struct FakeRunner {
        verbose_columns: Vec<String>,
        ran: Vec<String>,
        /// `prepare <save>` and `run <save>`, in call order.
        events: Vec<String>,
    }

    impl FakeRunner {
        fn new() -> Self {
            Self {
                verbose_columns: vec![
                    "timestamp".into(),
                    "wholeUpdate".into(),
                    "entityUpdate".into(),
                ],
                ran: Vec::new(),
                events: Vec::new(),
            }
        }
    }

    impl BenchmarkRunner for FakeRunner {
        /// A save named `unsyncable` can't get its mods.
        fn prepare(&mut self, save: &Save) -> Result<(), RunFailure> {
            self.events.push(format!("prepare {}", save.name));
            match save.name.as_str() {
                "unsyncable" => Err(RunFailure::new(FailureReason::Mods("missing mods".into()))),
                _ => Ok(()),
            }
        }

        fn benchmark(
            &mut self,
            save: &Save,
            request: &BenchmarkRequest,
        ) -> Result<Measurement, RunFailure> {
            self.ran.push(save.name.clone());
            self.events.push(format!("run {}", save.name));
            match save.name.as_str() {
                "broken" => {
                    return Err(RunFailure::with_tail(
                        FailureReason::Exited(Some(1)),
                        vec!["boom".into()],
                    ));
                }
                "stop" => return Err(RunFailure::new(FailureReason::Interrupted)),
                _ => {}
            }
            let verbose = request.verbose.then(|| VerboseTimings {
                columns: self.verbose_columns.clone(),
                rows: vec![
                    VerboseRow {
                        tick: 0,
                        values: vec![1, 9_000_000, 5],
                    },
                    VerboseRow {
                        tick: 1,
                        values: vec![2, 3_000_000, 6],
                    },
                    VerboseRow {
                        tick: 2,
                        values: vec![3, 4_000_000, 7],
                    },
                ],
            });
            Ok(Measurement {
                execution_time_ms: 1000.0,
                avg_ms: 1.0,
                min_ms: 0.5,
                max_ms: 2.0,
                ticks: request.ticks,
                factorio_version: "2.1.25".into(),
                verbose,
                mimalloc: None,
                uprof: None,
            })
        }
    }

    struct FakeCpu;

    impl CpuMonitor for FakeCpu {
        fn start(&self) -> Box<dyn CpuRecording> {
            Box::new(FakeCpu)
        }
    }

    impl CpuRecording for FakeCpu {
        fn stop(self: Box<Self>) -> Vec<CpuSample> {
            vec![CpuSample {
                core_index: 0,
                mhz: 4000,
                ms_since_start: 0,
            }]
        }
    }

    fn settings(dir: &Path) -> BenchmarkSettings {
        BenchmarkSettings {
            ticks: 600,
            runs: 2,
            run_order: RunOrder::Grouped,
            output: dir.to_path_buf(),
            ..Default::default()
        }
    }

    fn saves(names: &[&str]) -> Vec<Save> {
        names
            .iter()
            .map(|name| Save::new(PathBuf::from(format!("/saves/{name}.zip"))))
            .collect()
    }

    fn run_with(
        settings: &BenchmarkSettings,
        saves: &[Save],
        runner: &mut FakeRunner,
    ) -> Result<Outcome, BenchmarkError> {
        run(
            settings,
            saves,
            runner,
            &FakeCpu,
            "2026-11-24",
            &mut fastrand::Rng::with_seed(1),
        )
    }

    fn results(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(output::RESULTS_FILE)).unwrap()
    }

    #[test]
    fn a_failed_save_does_not_stop_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = FakeRunner::new();

        let outcome = run_with(
            &settings(dir.path()),
            &saves(&["broken", "good"]),
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            (runner.ran, outcome.failures.is_empty()),
            (
                vec!["broken".to_string(), "good".into(), "good".into()],
                false
            )
        );
    }

    #[test]
    fn an_interrupt_keeps_completed_runs() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.runs = 1;

        let outcome = run_with(
            &settings,
            &saves(&["good", "stop", "later"]),
            &mut FakeRunner::new(),
        )
        .unwrap();

        let rows: Vec<String> = results(dir.path())
            .lines()
            .skip(1)
            .map(|line| line.split(',').next().unwrap().to_string())
            .collect();
        assert_eq!(
            (outcome.interrupted, rows),
            (true, vec!["good".to_string()])
        );
    }

    #[test]
    fn a_strip_prefix_is_removed_from_reported_save_names() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        (settings.strip_prefix, settings.runs) = (Some("bench-".into()), 1);

        run_with(
            &settings,
            &saves(&["bench-smelting"]),
            &mut FakeRunner::new(),
        )
        .unwrap();

        assert!(
            results(dir.path())
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("smelting,0,")
        );
    }

    #[test]
    fn append_continues_run_indexes_from_existing_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.runs = 1;
        run_with(&settings, &saves(&["a"]), &mut FakeRunner::new()).unwrap();
        settings.append = true;

        run_with(&settings, &saves(&["a"]), &mut FakeRunner::new()).unwrap();

        let text = results(dir.path());
        let indexes: Vec<&str> = text
            .lines()
            .skip(1)
            .map(|line| line.split(',').nth(1).unwrap())
            .collect();
        assert_eq!(indexes, vec!["0", "1"]);
    }

    #[test]
    fn append_recomputes_differences_over_all_rows() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(output::RESULTS_FILE),
            format!(
                "{}\nold,0,2000,1,0.5,2,300,0,600,2.1.25,linux-x86_64\n",
                output::RESULTS_HEADER
            ),
        )
        .unwrap();
        let mut settings = settings(dir.path());
        (settings.append, settings.runs) = (true, 1);

        run_with(&settings, &saves(&["new"]), &mut FakeRunner::new()).unwrap();

        let text = results(dir.path());
        let new_row = text.lines().find(|line| line.starts_with("new,")).unwrap();
        assert_eq!(new_row.split(',').nth(7).unwrap(), "100");
    }

    #[test]
    fn append_refuses_an_existing_results_csv_with_another_header_before_the_first_run() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(output::RESULTS_FILE), "save,ups\n").unwrap();
        let mut settings = settings(dir.path());
        settings.append = true;
        let mut runner = FakeRunner::new();

        let result = run_with(&settings, &saves(&["a"]), &mut runner);

        assert!(
            matches!(
                result,
                Err(BenchmarkError::Output(OutputError::HeaderMismatch { .. }))
            ) && runner.ran.is_empty()
        );
    }

    #[test]
    fn an_unknown_verbose_metric_fails_after_the_first_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.verbose_metrics = vec!["entityUpdate".into(), "bogus".into()];
        let mut runner = FakeRunner::new();

        let result = run_with(&settings, &saves(&["a", "b"]), &mut runner);

        assert!(
            matches!(result, Err(BenchmarkError::UnknownMetrics(names)) if names == ["bogus"])
                && runner.ran.len() == 1
        );
    }

    #[test]
    fn verbose_all_keeps_every_factorio_column_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        (settings.verbose_metrics, settings.runs) = (vec!["all".into()], 1);
        let mut runner = FakeRunner::new();
        runner.verbose_columns = vec![
            "wholeUpdate".into(),
            "timestamp".into(),
            "entityUpdate".into(),
        ];

        run_with(&settings, &saves(&["a"]), &mut runner).unwrap();

        let header = std::fs::read_to_string(dir.path().join("a_verbose_metrics.csv"))
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        assert_eq!(header, "tick,run,wholeUpdate,entityUpdate,");
    }

    #[test]
    fn requested_metrics_are_written_in_factorio_column_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        (settings.verbose_metrics, settings.runs) =
            (vec!["entityUpdate".into(), "wholeUpdate".into()], 1);

        run_with(&settings, &saves(&["a"]), &mut FakeRunner::new()).unwrap();

        let header = std::fs::read_to_string(dir.path().join("a_verbose_metrics.csv"))
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        assert_eq!(header, "tick,run,wholeUpdate,entityUpdate");
    }

    #[test]
    fn the_first_tick_is_written_as_zero() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        (settings.verbose_metrics, settings.runs) = (vec!["entityUpdate".into()], 1);

        run_with(&settings, &saves(&["a"]), &mut FakeRunner::new()).unwrap();

        let text = std::fs::read_to_string(dir.path().join("a_verbose_metrics.csv")).unwrap();
        assert_eq!(text.lines().nth(1).unwrap(), "0,0,5");
    }

    #[test]
    fn max_ms_is_the_highest_whole_update_after_the_first_tick_when_verbose() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        (settings.verbose_metrics, settings.runs) = (vec!["entityUpdate".into()], 1);

        run_with(&settings, &saves(&["a"]), &mut FakeRunner::new()).unwrap();

        assert_eq!(
            results(dir.path())
                .lines()
                .nth(1)
                .unwrap()
                .split(',')
                .nth(5)
                .unwrap(),
            "4"
        );
    }

    #[test]
    fn every_save_is_prepared_before_the_first_timed_run() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.runs = 1;
        let mut runner = FakeRunner::new();

        run_with(&settings, &saves(&["a", "b"]), &mut runner).unwrap();

        assert_eq!(
            runner.events,
            vec!["prepare a", "prepare b", "run a", "run b"]
        );
    }

    #[test]
    fn a_save_whose_mods_cannot_be_prepared_fails_without_running() {
        let dir = tempfile::tempdir().unwrap();
        let mut runner = FakeRunner::new();

        let outcome = run_with(
            &settings(dir.path()),
            &saves(&["unsyncable", "good"]),
            &mut runner,
        )
        .unwrap();

        assert_eq!(
            (outcome.failures.is_empty(), runner.ran),
            (false, vec!["good".to_string(), "good".into()])
        );
    }

    #[test]
    fn timestamp_is_never_a_metric_column() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = settings(dir.path());
        settings.verbose_metrics = vec!["wholeUpdate".into(), "timestamp".into()];
        let mut runner = FakeRunner::new();

        let result = run_with(&settings, &saves(&["a"]), &mut runner);

        assert!(
            matches!(result, Err(BenchmarkError::NotMetrics(names)) if names == ["timestamp"])
                && runner.ran.is_empty()
        );
    }
}
