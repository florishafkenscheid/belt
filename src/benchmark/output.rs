//! The benchmark output files of `contract.md`: `results.csv`, `cpu_freq.csv`, the verbose
//! metric files and `results.md`. Numbers are written in `f64`'s `Display` form.

use std::collections::{BTreeMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// One row per run (`contract.md`).
pub const RESULTS_FILE: &str = "results.csv";
/// The exact header of `results.csv`; appending refuses a file with any other.
pub const RESULTS_HEADER: &str = "save_name,run_index,execution_time_ms,avg_ms,min_ms,max_ms,effective_ups,percentage_improvement,ticks,factorio_version,platform";
/// CPU frequency samples of every run, one shared file.
pub const CPU_FILE: &str = "cpu_freq.csv";
/// The exact header of `cpu_freq.csv`; appending refuses a file with any other.
pub const CPU_HEADER: &str = "save_name,run_index,core_index,cpu_frequency,timestamp";

/// Why an output file can't be read, extended or written. Every variant names the file.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Csv { path: PathBuf, source: csv::Error },
    #[error("{path} has header {found:?}, expected {expected:?}; refusing to append")]
    HeaderMismatch {
        path: PathBuf,
        found: String,
        expected: String,
    },
    #[error("{path}: {message}")]
    Malformed { path: PathBuf, message: String },
    #[error(
        "{existing} exists; refusing to write {wanted} next to it (compress must match the existing file)"
    )]
    CompressMismatch { existing: PathBuf, wanted: PathBuf },
    #[error("{path} is not a complete zstd stream ({message}); refusing to append to it")]
    Truncated { path: PathBuf, message: String },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> OutputError + '_ {
    move |source| OutputError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn csv_error(path: &Path) -> impl FnOnce(csv::Error) -> OutputError + '_ {
    move |source| OutputError::Csv {
        path: path.to_path_buf(),
        source,
    }
}

/// One row of `results.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRecord {
    pub save_name: String,
    pub run_index: u32,
    pub execution_time_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub effective_ups: f64,
    pub percentage_improvement: f64,
    pub ticks: u32,
    pub factorio_version: String,
    pub platform: String,
}

impl RunRecord {
    fn fields(&self) -> [String; 11] {
        [
            self.save_name.clone(),
            self.run_index.to_string(),
            self.execution_time_ms.to_string(),
            self.avg_ms.to_string(),
            self.min_ms.to_string(),
            self.max_ms.to_string(),
            self.effective_ups.to_string(),
            self.percentage_improvement.to_string(),
            self.ticks.to_string(),
            self.factorio_version.clone(),
            self.platform.clone(),
        ]
    }

    fn from_fields(record: &csv::StringRecord) -> Result<Self, String> {
        let text = |i: usize| {
            record
                .get(i)
                .ok_or_else(|| format!("row has {} columns", record.len()))
        };
        let number = |i: usize| -> Result<f64, String> {
            text(i)?
                .parse()
                .map_err(|_| format!("{:?} is not a number", text(i)))
        };
        let count = |i: usize| -> Result<u32, String> {
            text(i)?
                .parse()
                .map_err(|_| format!("{:?} is not a count", text(i)))
        };
        Ok(Self {
            save_name: text(0)?.into(),
            run_index: count(1)?,
            execution_time_ms: number(2)?,
            avg_ms: number(3)?,
            min_ms: number(4)?,
            max_ms: number(5)?,
            effective_ups: number(6)?,
            percentage_improvement: number(7)?,
            ticks: count(8)?,
            factorio_version: text(9)?.into(),
            platform: text(10)?.into(),
        })
    }
}

/// `1000 × ticks / execution_time_ms`.
pub fn effective_ups(ticks: u32, execution_time_ms: f64) -> f64 {
    1000.0 * f64::from(ticks) / execution_time_ms
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0u32), |(sum, count), value| (sum + value, count + 1));
    sum / f64::from(count)
}

fn mean_ups_by_save(records: &[RunRecord]) -> BTreeMap<&str, f64> {
    let mut by_save: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for record in records {
        by_save
            .entry(&record.save_name)
            .or_default()
            .push(record.effective_ups);
    }
    by_save
        .into_iter()
        .map(|(save, ups)| (save, mean(ups.into_iter())))
        .collect()
}

/// Sets every row's `percentage_improvement` against the slowest save's mean UPS.
pub fn set_differences(records: &mut [RunRecord]) {
    let means: BTreeMap<String, f64> = mean_ups_by_save(records)
        .into_iter()
        .map(|(save, ups)| (save.to_string(), ups))
        .collect();
    let lowest = means.values().copied().fold(f64::INFINITY, f64::min);
    for record in records.iter_mut() {
        let save_mean = means[&record.save_name];
        record.percentage_improvement = if lowest == 0.0 {
            0.0
        } else {
            (save_mean - lowest) / lowest * 100.0
        };
    }
}

/// `results.csv` order: grouped by save, saves by ascending mean UPS (then name), rows by run
/// index.
pub fn in_csv_order(mut records: Vec<RunRecord>) -> Vec<RunRecord> {
    let means: BTreeMap<String, f64> = mean_ups_by_save(&records)
        .into_iter()
        .map(|(save, ups)| (save.to_string(), ups))
        .collect();
    records.sort_by(|left, right| {
        let by_mean = means[&left.save_name].total_cmp(&means[&right.save_name]);
        by_mean
            .then_with(|| left.save_name.cmp(&right.save_name))
            .then(left.run_index.cmp(&right.run_index))
    });
    records
}

/// Fails unless the file at `path` starts with `expected`. A missing file passes.
pub fn check_header(path: &Path, expected: &str) -> Result<(), OutputError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(OutputError::Io {
                path: path.into(),
                source,
            });
        }
    };
    let found = first_line(BufReader::new(file)).map_err(io(path))?;
    if found != expected {
        return Err(OutputError::HeaderMismatch {
            path: path.into(),
            found,
            expected: expected.into(),
        });
    }
    Ok(())
}

fn first_line(mut reader: impl BufRead) -> std::io::Result<String> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

/// The rows of an existing `results.csv`, in file order. A missing file has none.
pub fn read_results(path: &Path) -> Result<Vec<RunRecord>, OutputError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    check_header(path, RESULTS_HEADER)?;
    let mut reader = csv::Reader::from_path(path).map_err(csv_error(path))?;
    reader
        .records()
        .map(|record| {
            let record = record.map_err(csv_error(path))?;
            RunRecord::from_fields(&record).map_err(|message| OutputError::Malformed {
                path: path.into(),
                message,
            })
        })
        .collect()
}

/// Replaces `results.csv` with the header and `records`, in the order given.
pub fn write_results(path: &Path, records: &[RunRecord]) -> Result<(), OutputError> {
    let mut writer = csv::Writer::from_path(path).map_err(csv_error(path))?;
    writer
        .write_record(RESULTS_HEADER.split(','))
        .map_err(csv_error(path))?;
    for record in records {
        writer
            .write_record(record.fields())
            .map_err(csv_error(path))?;
    }
    writer.flush().map_err(io(path))
}

/// One CPU frequency sample of one core.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuRow {
    pub save_name: String,
    pub run_index: u32,
    pub core_index: usize,
    pub mhz: u64,
    pub ms_since_start: u64,
}

/// What a command's first write does with an output file that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Existing {
    /// Start the file over.
    Replace,
    /// Keep its rows and add to them.
    Append,
}

/// How verbose metric files are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Plain,
    /// One zstd frame per write, so appending never recompresses earlier data.
    Zstd,
}

impl Compression {
    fn other(self) -> Self {
        match self {
            Self::Plain => Self::Zstd,
            Self::Zstd => Self::Plain,
        }
    }
}

/// Adds rows to `cpu_freq.csv`. With [`Existing::Replace`], or when the file is missing, it is
/// started over with its header.
pub fn append_cpu_rows(
    path: &Path,
    rows: &[CpuRow],
    existing: Existing,
) -> Result<(), OutputError> {
    let new_file = existing == Existing::Replace || !path.exists();
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!new_file)
        .truncate(new_file)
        .open(path)
        .map_err(io(path))?;
    let mut writer = csv::Writer::from_writer(file);
    if new_file {
        writer
            .write_record(CPU_HEADER.split(','))
            .map_err(csv_error(path))?;
    }
    for row in rows {
        let fields = [
            row.save_name.clone(),
            row.run_index.to_string(),
            row.core_index.to_string(),
            row.mhz.to_string(),
            row.ms_since_start.to_string(),
        ];
        writer.write_record(fields).map_err(csv_error(path))?;
    }
    writer.flush().map_err(io(path))
}

/// Which of Factorio's verbose columns a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// Only the requested metrics.
    Requested,
    /// Every column Factorio reports, its empty last one included: Factorio's header ends in a
    /// comma, so the file's header and every row do too.
    All,
}

/// Verbose metric timings of one run, already reduced to the metric columns to write.
#[derive(Debug, Clone, PartialEq)]
pub struct VerboseTable {
    pub metrics: Vec<String>,
    /// Per tick: the tick number and one value per metric, in nanoseconds.
    pub rows: Vec<(u64, Vec<u64>)>,
    pub selection: Selection,
}

/// Verbose files hold Factorio's values in nanoseconds, unchanged.
impl VerboseTable {
    fn header(&self) -> String {
        let columns = ["tick", "run"]
            .into_iter()
            .map(String::from)
            .chain(self.metrics.iter().cloned());
        let header = columns.collect::<Vec<_>>().join(",");
        if self.selection == Selection::All {
            format!("{header},")
        } else {
            header
        }
    }

    fn csv_rows(&self, run: u32) -> Vec<u8> {
        let mut writer = csv::Writer::from_writer(Vec::new());
        for (tick, values) in &self.rows {
            let nanoseconds = values.iter().map(u64::to_string);
            let mut fields: Vec<String> = [tick.to_string(), run.to_string()]
                .into_iter()
                .chain(nanoseconds)
                .collect();
            if self.selection == Selection::All {
                fields.push(String::new());
            }
            writer.write_record(&fields).expect("writing to memory");
        }
        writer.into_inner().expect("writing to memory")
    }
}

/// The per-save verbose metric files of one command.
pub struct VerboseFiles {
    dir: PathBuf,
    compression: Compression,
    existing: Existing,
    /// Saves whose file this command has already started.
    started: HashSet<String>,
}

impl VerboseFiles {
    /// The files in `dir`. `existing` decides what a save's first write does with a file from
    /// an earlier command.
    pub fn new(dir: &Path, compression: Compression, existing: Existing) -> Self {
        Self {
            dir: dir.to_path_buf(),
            compression,
            existing,
            started: HashSet::new(),
        }
    }

    fn path(&self, save_name: &str, compression: Compression) -> PathBuf {
        let extension = match compression {
            Compression::Plain => "csv",
            Compression::Zstd => "csv.zst",
        };
        self.dir
            .join(format!("{save_name}_verbose_metrics.{extension}"))
    }

    /// Checks, before any run, that no save has a verbose file of the other kind, and that
    /// compressed files to append to decode completely.
    pub fn check(&self, save_names: &[String]) -> Result<(), OutputError> {
        for save_name in save_names {
            let wanted = self.path(save_name, self.compression);
            let other = self.path(save_name, self.compression.other());
            if other.exists() {
                return Err(OutputError::CompressMismatch {
                    existing: other,
                    wanted,
                });
            }
            if self.existing == Existing::Append
                && self.compression == Compression::Zstd
                && wanted.exists()
            {
                verify_zstd(&wanted)?;
            }
        }
        Ok(())
    }

    /// Writes one run's rows. The save's file is replaced at its first run in this command,
    /// unless appending; any later write first checks that the file's header matches.
    pub fn write_run(
        &mut self,
        save_name: &str,
        run: u32,
        table: &VerboseTable,
    ) -> Result<(), OutputError> {
        let path = self.path(save_name, self.compression);
        let header = table.header();
        let continuing = self.started.contains(save_name)
            || (self.existing == Existing::Append && path.exists());
        if continuing {
            let found = self.existing_header(&path)?;
            if found != header {
                return Err(OutputError::HeaderMismatch {
                    path,
                    found,
                    expected: header,
                });
            }
        }
        let mut bytes = Vec::new();
        if !continuing {
            bytes.extend(format!("{header}\n").into_bytes());
        }
        bytes.extend(table.csv_rows(run));
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(continuing)
            .truncate(!continuing)
            .open(&path)
            .map_err(io(&path))?;
        match self.compression {
            Compression::Zstd => {
                let mut encoder = zstd::Encoder::new(file, 0).map_err(io(&path))?;
                encoder.write_all(&bytes).map_err(io(&path))?;
                encoder.finish().map_err(io(&path))?;
            }
            Compression::Plain => (&file).write_all(&bytes).map_err(io(&path))?,
        }
        self.started.insert(save_name.to_string());
        Ok(())
    }

    fn existing_header(&self, path: &Path) -> Result<String, OutputError> {
        let file = File::open(path).map_err(io(path))?;
        match self.compression {
            Compression::Zstd => {
                first_line(BufReader::new(zstd::Decoder::new(file).map_err(io(path))?))
                    .map_err(io(path))
            }
            Compression::Plain => first_line(BufReader::new(file)).map_err(io(path)),
        }
    }
}

fn verify_zstd(path: &Path) -> Result<(), OutputError> {
    let file = File::open(path).map_err(io(path))?;
    let mut decoder = zstd::Decoder::new(file).map_err(io(path))?;
    std::io::copy(&mut decoder, &mut std::io::sink())
        .map(|_| ())
        .map_err(|error| OutputError::Truncated {
            path: path.into(),
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn decompress(path: &Path) -> String {
        let mut text = String::new();
        zstd::Decoder::new(File::open(path).unwrap())
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }

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

    fn table(metric: &str) -> VerboseTable {
        VerboseTable {
            metrics: vec![metric.into()],
            rows: vec![(0, vec![2_500_000]), (1, vec![500_000])],
            selection: Selection::Requested,
        }
    }

    #[test]
    fn effective_ups_is_ticks_per_second_of_execution_time() {
        assert_eq!(effective_ups(6000, 2000.0), 3000.0);
    }

    #[test]
    fn the_slowest_save_is_the_base_and_differences_are_relative_to_it() {
        let mut records = vec![record("fast", 0, 150.0), record("slow", 0, 100.0)];

        set_differences(&mut records);

        let differences: Vec<f64> = records
            .iter()
            .map(|record| record.percentage_improvement)
            .collect();
        assert_eq!(differences, vec![50.0, 0.0]);
    }

    #[test]
    fn a_lowest_mean_of_zero_gives_no_difference() {
        let mut records = vec![record("zero", 0, 0.0), record("fast", 0, 150.0)];

        set_differences(&mut records);

        assert_eq!(records[1].percentage_improvement, 0.0);
    }

    #[test]
    fn results_csv_has_the_contract_header_and_one_row_per_run() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RESULTS_FILE);
        let mut quoted = record("a, b", 0, 100.0);
        quoted.execution_time_ms = 60.5;

        write_results(&path, &[quoted, record("c", 1, 3.0)]).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                "{RESULTS_HEADER}\n\"a, b\",0,60.5,1,0.5,2,100,0,6000,2.1.25,linux-x86_64\nc,1,1000,1,0.5,2,3,0,6000,2.1.25,linux-x86_64\n"
            )
        );
    }

    #[test]
    fn results_csv_rows_are_grouped_by_save_in_ascending_mean_ups() {
        let records = vec![
            record("b", 0, 200.0),
            record("a", 0, 300.0),
            record("b", 1, 220.0),
            record("a", 1, 310.0),
        ];

        let ordered: Vec<(String, u32)> = in_csv_order(records)
            .into_iter()
            .map(|record| (record.save_name, record.run_index))
            .collect();

        assert_eq!(
            ordered,
            vec![
                ("b".into(), 0),
                ("b".into(), 1),
                ("a".into(), 0),
                ("a".into(), 1)
            ]
        );
    }

    #[test]
    fn cpu_freq_csv_is_one_shared_file_with_the_contract_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CPU_FILE);
        let row = |save: &str| CpuRow {
            save_name: save.into(),
            run_index: 0,
            core_index: 1,
            mhz: 4500,
            ms_since_start: 100,
        };

        append_cpu_rows(&path, &[row("a")], Existing::Replace).unwrap();
        append_cpu_rows(&path, &[row("b")], Existing::Append).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{CPU_HEADER}\na,0,1,4500,100\nb,0,1,4500,100\n")
        );
    }

    #[test]
    fn append_keeps_existing_cpu_frequency_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CPU_FILE);
        std::fs::write(&path, format!("{CPU_HEADER}\nold,0,0,4000,0\n")).unwrap();
        let row = CpuRow {
            save_name: "new".into(),
            run_index: 1,
            core_index: 0,
            mhz: 4100,
            ms_since_start: 0,
        };

        append_cpu_rows(&path, &[row], Existing::Append).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{CPU_HEADER}\nold,0,0,4000,0\nnew,1,0,4100,0\n")
        );
    }

    #[test]
    fn verbose_metrics_file_is_named_after_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = VerboseFiles::new(dir.path(), Compression::Plain, Existing::Replace);

        files
            .write_run("my-save", 0, &table("wholeUpdate"))
            .unwrap();

        assert!(dir.path().join("my-save_verbose_metrics.csv").is_file());
    }

    #[test]
    fn all_output_keeps_factorios_empty_last_column() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = VerboseFiles::new(dir.path(), Compression::Plain, Existing::Replace);
        let all = VerboseTable {
            selection: Selection::All,
            ..table("wholeUpdate")
        };

        files.write_run("s", 0, &all).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("s_verbose_metrics.csv")).unwrap(),
            "tick,run,wholeUpdate,\n0,0,2500000,\n1,0,500000,\n"
        );
    }

    #[test]
    fn verbose_timings_are_written_in_factorio_nanoseconds() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = VerboseFiles::new(dir.path(), Compression::Plain, Existing::Replace);

        files.write_run("s", 3, &table("wholeUpdate")).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("s_verbose_metrics.csv")).unwrap(),
            "tick,run,wholeUpdate\n0,3,2500000\n1,3,500000\n"
        );
    }

    #[test]
    fn append_checks_the_verbose_header_after_each_run() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("s_verbose_metrics.csv"),
            "tick,run,entityUpdate\n0,0,1\n",
        )
        .unwrap();
        let mut files = VerboseFiles::new(dir.path(), Compression::Plain, Existing::Append);

        let result = files.write_run("s", 1, &table("wholeUpdate"));

        assert!(matches!(result, Err(OutputError::HeaderMismatch { .. })));
    }

    #[test]
    fn compress_writes_verbose_metrics_as_zstd_and_other_csvs_plain() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = VerboseFiles::new(dir.path(), Compression::Zstd, Existing::Replace);

        files.write_run("s", 0, &table("wholeUpdate")).unwrap();

        assert_eq!(
            decompress(&dir.path().join("s_verbose_metrics.csv.zst")),
            "tick,run,wholeUpdate\n0,0,2500000\n1,0,500000\n"
        );
    }

    #[test]
    fn appending_to_a_compressed_file_adds_a_frame_without_recompressing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s_verbose_metrics.csv.zst");
        VerboseFiles::new(dir.path(), Compression::Zstd, Existing::Replace)
            .write_run("s", 0, &table("wholeUpdate"))
            .unwrap();
        let first_frame = std::fs::read(&path).unwrap();

        VerboseFiles::new(dir.path(), Compression::Zstd, Existing::Append)
            .write_run("s", 1, &table("wholeUpdate"))
            .unwrap();

        assert!(std::fs::read(&path).unwrap().starts_with(&first_frame));
    }

    #[test]
    fn compress_refuses_a_plain_verbose_file_next_to_a_compressed_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s_verbose_metrics.csv.zst"), "").unwrap();

        let result = VerboseFiles::new(dir.path(), Compression::Plain, Existing::Append)
            .check(&["s".into()]);

        assert!(matches!(result, Err(OutputError::CompressMismatch { .. })));
    }

    #[test]
    fn append_refuses_a_truncated_compressed_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s_verbose_metrics.csv.zst");
        VerboseFiles::new(dir.path(), Compression::Zstd, Existing::Replace)
            .write_run("s", 0, &table("wholeUpdate"))
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 4]).unwrap();

        let result =
            VerboseFiles::new(dir.path(), Compression::Zstd, Existing::Append).check(&["s".into()]);

        assert!(matches!(result, Err(OutputError::Truncated { .. })));
    }
}
