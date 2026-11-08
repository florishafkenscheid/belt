use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, Error, ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

use csv::StringRecord;

use crate::{
    benchmark::{parser::BenchmarkRun, runner::VerboseData},
    core::{
        config::BenchmarkConfig,
        error::{BenchmarkErrorKind, Result},
        output::{ResultWriter, WriteData, ensure_output_dir},
    },
};

pub struct CsvWriter {}

impl Default for CsvWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl ResultWriter for CsvWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Benchmark(data) => write_benchmark_csv(data, path),
            WriteData::Verbose {
                data,
                metrics_to_export,
                compress,
            } => write_verbose_csv(data, metrics_to_export, *compress, false, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }

    fn append(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::Benchmark(data) => append_benchmark_csv(data, path),
            WriteData::Verbose {
                data,
                metrics_to_export,
                compress,
            } => write_verbose_csv(data, metrics_to_export, *compress, true, path),
            _ => Err(BenchmarkErrorKind::InvalidWriteData.into()),
        }
    }
}

/// Write the results to a CSV file
fn write_benchmark_csv(results: &[BenchmarkRun], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    let csv_path = path.join("results.csv");

    let mut writer = csv::Writer::from_path(&csv_path)?;

    writer.write_record([
        "save_name",
        "run_index",
        "execution_time_ms",
        "avg_ms",
        "min_ms",
        "max_ms",
        "effective_ups",
        "percentage_improvement",
        "ticks",
        "factorio_version",
        "platform",
    ])?;

    for result in results {
        writer.write_record([
            &result.save_name,
            &result.index.to_string(),
            &result.execution_time_ms.to_string(),
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.effective_ups.to_string(),
            &result.base_diff.to_string(),
            &result.ticks.to_string(),
            &result.factorio_version,
            &result.platform,
        ])?;
    }

    writer.flush()?;
    tracing::info!("Results written to {}", csv_path.display());

    write_cpu_freq_csv(results, path)?;

    Ok(())
}

/// Write or append factorio's verbose output to the save's verbose metrics CSV
fn write_verbose_csv(
    data: &[VerboseData],
    metrics: &[String],
    compress: bool,
    append: bool,
    path: &Path,
) -> Result<()> {
    ensure_output_dir(path)?;

    let Some(first_run) = data.first() else {
        return Ok(());
    };

    let mut reader = csv::Reader::from_reader(first_run.csv_data.as_bytes());
    let metrics_to_export = resolve_verbose_metrics(reader.headers()?, metrics)?;
    let header = verbose_header(&metrics_to_export);

    let csv_path = verbose_metrics_path(path, &first_run.save_name, compress);
    let append = append && csv_path.exists();
    let first_run_index = if append {
        validate_csv_header(&csv_path, &header)?;
        next_verbose_run_index(&csv_path)?
    } else {
        0
    };

    let mut writer = csv::Writer::from_writer(CsvSink::open(&csv_path, append)?);
    if !append {
        writer.write_record(&header)?;
    }

    for (offset, run_data) in data.iter().enumerate() {
        let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
        let columns = metric_indexes(inner_reader.headers()?, &metrics_to_export)?;
        let run_index = first_run_index + offset as u32;

        for record_result in inner_reader.records() {
            let record = record_result?;

            let tick_str = record.get(0).unwrap_or("t0");
            let tick_value = tick_str.trim_start_matches('t');

            let mut data_row = vec![tick_value.to_string(), run_index.to_string()];
            // The csv reader rejects rows that don't match the header length, so every index exists
            data_row.extend(columns.iter().map(|&index| record[index].to_string()));

            writer.write_record(data_row)?;
        }
    }

    finish_csv(writer)?;
    tracing::debug!(
        "Verbose metrics for {} {} {}",
        first_run.save_name,
        if append { "appended to" } else { "exported to" },
        csv_path.display()
    );
    Ok(())
}

/// Checks a single run's verbose output against the requested metrics and, when appending, the
/// existing CSV. Called as each run finishes, so a bad metric name or header fails after one run
/// instead of after the whole benchmark.
pub fn check_verbose_run(
    run: &VerboseData,
    config: &BenchmarkConfig,
    output_dir: &Path,
) -> Result<()> {
    let mut reader = csv::Reader::from_reader(run.csv_data.as_bytes());
    let metrics = resolve_verbose_metrics(reader.headers()?, &config.verbose_metrics)?;

    let csv_path = verbose_metrics_path(output_dir, &run.save_name, config.compress);
    if config.append && csv_path.exists() {
        validate_csv_header(&csv_path, &verbose_header(&metrics))?;
    }

    Ok(())
}

/// Checks, before anything runs, that every CSV this benchmark writes can be written, so a
/// format conflict or malformed existing file can't fail a benchmark after hours of running.
pub fn preflight(
    config: &BenchmarkConfig,
    output_dir: &Path,
    verbose_save_names: &[String],
) -> Result<()> {
    if !config.verbose_metrics.is_empty() {
        // With "all" the header comes from Factorio, so check_verbose_run validates it instead
        let explicit_header = (!config.verbose_metrics.iter().any(|m| m == "all"))
            .then(|| verbose_header(&config.verbose_metrics));

        for save_name in verbose_save_names {
            let target = verbose_metrics_path(output_dir, save_name, config.compress);
            let other = verbose_metrics_path(output_dir, save_name, !config.compress);
            if other.exists() {
                return Err(BenchmarkErrorKind::CompressionConflict {
                    existing: other,
                    target,
                }
                .into());
            }

            if config.append && target.exists() {
                if let Some(header) = &explicit_header {
                    validate_csv_header(&target, header)?;
                }
                // Reads the whole file, so a truncated or corrupt one fails now
                next_verbose_run_index(&target)?;
            }
        }
    }

    if config.append {
        let results_path = output_dir.join("results.csv");
        if results_path.exists() {
            validate_csv_header(&results_path, &BENCHMARK_HEADER)?;
            next_benchmark_run_indexes(&results_path)?;
        }

        let cpu_freq_path = output_dir.join("cpu_freq.csv");
        if config.record_cpu && cpu_freq_path.exists() {
            validate_csv_header(&cpu_freq_path, &CPU_FREQ_HEADER)?;
        }
    }

    Ok(())
}

/// Path of a save's verbose metrics CSV, `.csv.zst` when compressed
pub fn verbose_metrics_path(output_dir: &Path, save_name: &str, compress: bool) -> PathBuf {
    let extension = if compress { "csv.zst" } else { "csv" };
    output_dir.join(format!("{save_name}_verbose_metrics.{extension}"))
}

/// Resolves `--verbose-metrics` against the header of factorio's verbose output
fn resolve_verbose_metrics(
    factorio_header: &StringRecord,
    metrics: &[String],
) -> Result<Vec<String>> {
    if metrics.iter().any(|m| m == "all") {
        return Ok(factorio_header
            .iter()
            .filter(|h| *h != "tick" && *h != "timestamp")
            .map(str::to_string)
            .collect());
    }

    metric_indexes(factorio_header, metrics)?;
    Ok(metrics.to_vec())
}

fn metric_indexes(factorio_header: &StringRecord, metrics: &[String]) -> Result<Vec<usize>> {
    metrics
        .iter()
        .map(|metric| {
            factorio_header
                .iter()
                .position(|h| h == metric)
                .ok_or_else(|| {
                    BenchmarkErrorKind::UnknownVerboseMetric {
                        metric: metric.clone(),
                        available: factorio_header
                            .iter()
                            .filter(|h| !h.is_empty())
                            .collect::<Vec<_>>()
                            .join(", "),
                    }
                    .into()
                })
        })
        .collect()
}

fn verbose_header(metrics: &[String]) -> Vec<String> {
    let mut header = vec!["tick".to_string(), "run".to_string()];
    header.extend(metrics.iter().cloned());
    header
}

fn is_zstd(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "zst")
}

fn csv_reader(path: &Path) -> Result<csv::Reader<Box<dyn Read>>> {
    let file = File::open(path)?;
    let source: Box<dyn Read> = if is_zstd(path) {
        Box::new(zstd::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    Ok(csv::Reader::from_reader(source))
}

/// File a CSV is written to. A zstd sink must be finished, or its last frame is left truncated.
enum CsvSink {
    Plain(File),
    Zstd(zstd::Encoder<'static, File>),
}

impl CsvSink {
    /// Appending to a zstd file adds a new frame, which decoders read as a continuation
    fn open(path: &Path, append: bool) -> Result<Self> {
        let file = if append {
            OpenOptions::new().append(true).open(path)?
        } else {
            File::create(path)?
        };

        Ok(if is_zstd(path) {
            Self::Zstd(zstd::Encoder::new(file, 0)?)
        } else {
            Self::Plain(file)
        })
    }

    fn finish(self) -> io::Result<()> {
        match self {
            Self::Plain(mut file) => file.flush(),
            Self::Zstd(encoder) => encoder.finish()?.flush(),
        }
    }
}

impl Write for CsvSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(file) => file.write(buf),
            Self::Zstd(encoder) => encoder.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(file) => file.flush(),
            Self::Zstd(encoder) => encoder.flush(),
        }
    }
}

fn finish_csv(writer: csv::Writer<CsvSink>) -> Result<()> {
    writer
        .into_inner()
        .map_err(|error| error.into_error())?
        .finish()?;
    Ok(())
}

fn write_cpu_freq_csv(data: &[BenchmarkRun], path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if let Some(first_run) = data.first()
        && first_run.cpu_data.is_empty()
    {
        tracing::debug!(
            "CPU frequency recording disabled or no CPU data captured. Skipping CPU frequency CSV."
        );
        return Ok(());
    }

    let csv_path = path.join("cpu_freq.csv");

    let mut writer = csv::Writer::from_path(&csv_path)?;

    writer.write_record([
        "save_name",
        "run_index",
        "core_index",
        "cpu_frequency",
        "timestamp",
    ])?;

    for result in data {
        for frequency_data in &result.cpu_data {
            writer.write_record([
                &result.save_name,
                &result.index.to_string(),
                &frequency_data.core_index.to_string(),
                &frequency_data.frequency.to_string(),
                &frequency_data.timestamp.to_string(),
            ])?;
        }
    }

    writer.flush()?;
    tracing::info!("CPU frequency results written to {}", csv_path.display());

    Ok(())
}

const BENCHMARK_HEADER: [&str; 11] = [
    "save_name",
    "run_index",
    "execution_time_ms",
    "avg_ms",
    "min_ms",
    "max_ms",
    "effective_ups",
    "percentage_improvement",
    "ticks",
    "factorio_version",
    "platform",
];

const CPU_FREQ_HEADER: [&str; 5] = [
    "save_name",
    "run_index",
    "core_index",
    "cpu_frequency",
    "timestamp",
];

fn append_benchmark_csv(results: &[BenchmarkRun], path: &Path) -> Result<()> {
    ensure_output_dir(path)?;

    let csv_path = path.join("results.csv");
    if !csv_path.exists() {
        return write_benchmark_csv(results, path);
    }

    validate_csv_header(&csv_path, &BENCHMARK_HEADER)?;

    let next_indexes = next_benchmark_run_indexes(&csv_path)?;
    let adjusted_results = offset_benchmark_run_indexes(results, &next_indexes);

    let file = OpenOptions::new().append(true).open(&csv_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for result in &adjusted_results {
        writer.write_record([
            &result.save_name,
            &result.index.to_string(),
            &result.execution_time_ms.to_string(),
            &result.avg_ms.to_string(),
            &result.min_ms.to_string(),
            &result.max_ms.to_string(),
            &result.effective_ups.to_string(),
            &result.base_diff.to_string(),
            &result.ticks.to_string(),
            &result.factorio_version,
            &result.platform,
        ])?;
    }

    writer.flush()?;
    tracing::info!("Results appended to {}", csv_path.display());

    append_cpu_freq_csv(&adjusted_results, path)?;

    Ok(())
}

fn append_cpu_freq_csv(data: &[BenchmarkRun], path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    if data.iter().all(|run| run.cpu_data.is_empty()) {
        return Ok(());
    }

    let csv_path = path.join("cpu_freq.csv");
    if !csv_path.exists() {
        return write_cpu_freq_csv(data, path);
    }

    validate_csv_header(&csv_path, &CPU_FREQ_HEADER)?;

    let file = OpenOptions::new().append(true).open(&csv_path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);

    for result in data {
        for frequency_data in &result.cpu_data {
            writer.write_record([
                &result.save_name,
                &result.index.to_string(),
                &frequency_data.core_index.to_string(),
                &frequency_data.frequency.to_string(),
                &frequency_data.timestamp.to_string(),
            ])?;
        }
    }

    writer.flush()?;
    tracing::info!("CPU frequency results appended to {}", csv_path.display());

    Ok(())
}

fn validate_csv_header<S>(csv_path: &Path, expected: &[S]) -> Result<()>
where
    S: AsRef<str>,
{
    let mut reader = csv_reader(csv_path)?;
    let actual = reader.headers()?;

    let expected: Vec<&str> = expected.iter().map(AsRef::as_ref).collect();

    if actual.iter().eq(expected.iter().copied()) {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::InvalidData,
            format!(
                "Cannot append to {}: header mismatch. Expected {:?}, found {:?}",
                csv_path.display(),
                expected,
                actual.iter().collect::<Vec<_>>()
            ),
        )
        .into())
    }
}

fn next_benchmark_run_indexes(csv_path: &Path) -> Result<HashMap<String, u32>> {
    let mut reader = csv::Reader::from_path(csv_path)?;
    let mut max_by_save: HashMap<String, u32> = HashMap::new();

    for record in reader.records() {
        let record = record?;
        let save_name = record.get(0).unwrap_or_default().to_string();
        let run_index = record.get(1).unwrap_or("0").parse::<u32>()?;

        max_by_save
            .entry(save_name)
            .and_modify(|max| *max = (*max).max(run_index))
            .or_insert(run_index);
    }

    Ok(max_by_save
        .into_iter()
        .map(|(save_name, max_index)| (save_name, max_index + 1))
        .collect())
}

fn offset_benchmark_run_indexes(
    results: &[BenchmarkRun],
    next_indexes: &HashMap<String, u32>,
) -> Vec<BenchmarkRun> {
    results
        .iter()
        .cloned()
        .map(|mut result| {
            let offset = next_indexes.get(&result.save_name).copied().unwrap_or(0);
            result.index += offset;
            result
        })
        .collect()
}

fn next_verbose_run_index(csv_path: &Path) -> Result<u32> {
    let mut reader = csv_reader(csv_path)?;
    let mut max_run: Option<u32> = None;

    for record in reader.records() {
        let record = record?;
        let run_index = record.get(1).unwrap_or("0").parse::<u32>()?;
        max_run = Some(max_run.map_or(run_index, |max| max.max(run_index)));
    }

    Ok(max_run.map_or(0, |max| max + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::runner::CpuFrequencyData;

    #[test]
    fn test_cpu_freq_csv_uses_shared_filename_for_all_saves() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();

        let data = vec![
            BenchmarkRun {
                save_name: "alpha".to_string(),
                index: 0,
                cpu_data: vec![CpuFrequencyData {
                    frequency: 5000,
                    timestamp: 1,
                    core_index: 0,
                }],
                ..Default::default()
            },
            BenchmarkRun {
                save_name: "beta".to_string(),
                index: 1,
                cpu_data: vec![CpuFrequencyData {
                    frequency: 5100,
                    timestamp: 2,
                    core_index: 1,
                }],
                ..Default::default()
            },
        ];

        write_cpu_freq_csv(&data, path).expect("write cpu csv");

        let csv_path = path.join("cpu_freq.csv");
        assert!(csv_path.exists(), "cpu_freq.csv should be created");

        let csv = std::fs::read_to_string(csv_path).expect("read cpu csv");
        assert!(csv.contains("alpha"));
        assert!(csv.contains("beta"));
    }

    // Header and rows as factorio prints them, including the trailing comma
    const FACTORIO_VERBOSE: &str = "tick,timestamp,wholeUpdate,gameUpdate,\n\
        t0,100,822461,524723,\n\
        t1,827590,208671,121719,";

    fn verbose_run(save_name: &str) -> VerboseData {
        VerboseData {
            save_name: save_name.to_string(),
            csv_data: FACTORIO_VERBOSE.to_string(),
        }
    }

    fn config(metrics: &[&str], compress: bool, append: bool) -> BenchmarkConfig {
        BenchmarkConfig {
            verbose_metrics: metrics.iter().map(|m| m.to_string()).collect(),
            compress,
            append,
            ..Default::default()
        }
    }

    fn decompress(path: &Path) -> String {
        let mut text = String::new();
        zstd::Decoder::new(File::open(path).expect("open zst"))
            .expect("decoder")
            .read_to_string(&mut text)
            .expect("decompress");
        text
    }

    #[test]
    fn test_compressed_verbose_append_continues_run_index() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let metrics = vec!["wholeUpdate".to_string()];

        write_verbose_csv(&[verbose_run("alpha")], &metrics, true, false, path).expect("write");
        write_verbose_csv(&[verbose_run("alpha")], &metrics, true, true, path).expect("append");

        let csv_path = path.join("alpha_verbose_metrics.csv.zst");
        assert!(!path.join("alpha_verbose_metrics.csv").exists());
        assert_eq!(
            decompress(&csv_path),
            "tick,run,wholeUpdate\n0,0,822461\n1,0,208671\n0,1,822461\n1,1,208671\n"
        );
    }

    #[test]
    fn test_verbose_all_keeps_factorio_columns() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();

        write_verbose_csv(
            &[verbose_run("alpha")],
            &["all".to_string()],
            false,
            false,
            path,
        )
        .expect("write");

        let csv = std::fs::read_to_string(path.join("alpha_verbose_metrics.csv")).expect("read");
        assert_eq!(
            csv,
            "tick,run,wholeUpdate,gameUpdate,\n0,0,822461,524723,\n1,0,208671,121719,\n"
        );
    }

    #[test]
    fn test_unknown_verbose_metric_fails_the_run_check() {
        let temp_dir = tempfile::tempdir().expect("temp dir");

        let error = check_verbose_run(
            &verbose_run("alpha"),
            &config(&["wholeUpdate", "wholeUpdat"], false, false),
            temp_dir.path(),
        )
        .expect_err("unknown metric");
        assert!(error.to_string().contains("'wholeUpdat'"));
    }

    #[test]
    fn test_verbose_run_check_rejects_append_header_mismatch() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        write_verbose_csv(
            &[verbose_run("alpha")],
            &["wholeUpdate".to_string()],
            false,
            false,
            path,
        )
        .expect("write");

        check_verbose_run(&verbose_run("alpha"), &config(&["all"], false, true), path)
            .expect_err("header mismatch");
    }

    #[test]
    fn test_preflight_rejects_mismatched_compression() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let saves = vec!["alpha".to_string()];

        std::fs::write(path.join("alpha_verbose_metrics.csv.zst"), b"").expect("write zst");
        for append in [false, true] {
            preflight(&config(&["all"], false, append), path, &saves).expect_err("plain over zst");
        }
        preflight(&config(&["all"], true, false), path, &saves).expect("zst over zst");

        std::fs::write(path.join("alpha_verbose_metrics.csv"), b"").expect("write csv");
        for append in [false, true] {
            preflight(&config(&["all"], true, append), path, &saves).expect_err("zst over plain");
        }
    }

    #[test]
    fn test_preflight_rejects_truncated_compressed_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let metrics = vec!["wholeUpdate".to_string()];
        write_verbose_csv(&[verbose_run("alpha")], &metrics, true, false, path).expect("write");

        let csv_path = path.join("alpha_verbose_metrics.csv.zst");
        let bytes = std::fs::read(&csv_path).expect("read");
        std::fs::write(&csv_path, &bytes[..bytes.len() - 4]).expect("truncate");

        preflight(
            &config(&["wholeUpdate"], true, true),
            path,
            &["alpha".to_string()],
        )
        .expect_err("truncated file");
    }

    #[test]
    fn test_cpu_freq_append_keeps_existing_rows() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path();
        let run = |save_name: &str| BenchmarkRun {
            save_name: save_name.to_string(),
            cpu_data: vec![CpuFrequencyData {
                frequency: 5000,
                timestamp: 1,
                core_index: 0,
            }],
            ..Default::default()
        };

        write_cpu_freq_csv(&[run("alpha")], path).expect("write");
        append_cpu_freq_csv(&[run("beta")], path).expect("append");

        let csv = std::fs::read_to_string(path.join("cpu_freq.csv")).expect("read");
        assert!(csv.contains("alpha"));
        assert!(csv.contains("beta"));
        assert!(!path.join("cpu_req.csv").exists());
    }
}
