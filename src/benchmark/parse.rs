//! Reading one `--benchmark` run (`--benchmark-runs 1`) from Factorio's stdout.
//!
//! Factorio prints, among its log lines:
//! ```text
//!    0.000 2026-11-24 20:50:18; Factorio 2.1.25 (build 88022, linux64, headless, space-age)
//!   Performed 50 updates in 3.563 ms
//!   avg: 0.071 ms, min: 0.047 ms, max: 0.199 ms
//! run 1:
//! tick,wholeUpdate,entityUpdate,
//! t0,199375,39304,
//! ```
//! Verbose values are nanoseconds. Every verbose line ends with a comma.

use super::mimalloc::{self, MimallocStats};
use super::uprof::UprofRun;

/// What Factorio reported for one run.
#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    pub execution_time_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub ticks: u32,
    pub factorio_version: String,
    /// Present when verbose timings were requested.
    pub verbose: Option<VerboseTimings>,
    /// Present when the process printed mimalloc's heap statistics.
    pub mimalloc: Option<MimallocStats>,
    /// AMD uProf breadcrumbs, when a wrapper script printed them.
    pub uprof: Option<UprofRun>,
}

/// Per-tick timings in Factorio's column order. `columns` leaves out Factorio's own `tick`.
#[derive(Debug, Clone, PartialEq)]
pub struct VerboseTimings {
    pub columns: Vec<String>,
    pub rows: Vec<VerboseRow>,
}

/// One tick of verbose timings: the tick number without Factorio's `t` prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct VerboseRow {
    pub tick: u64,
    /// Nanoseconds, one per column.
    pub values: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
struct TickTimes {
    avg_ms: f64,
    min_ms: f64,
    max_ms: f64,
}

/// Feeds on stdout lines as they arrive, so only what the benchmark needs is kept.
#[derive(Debug, Default)]
pub struct BenchmarkParser {
    version: Option<String>,
    performed: Option<(u32, f64)>,
    tick_times: Option<TickTimes>,
    verbose: Option<VerboseTimings>,
    /// The output from the last `heap stats:` line on, up to mimalloc's statistics' length.
    heap_stats: Vec<String>,
    uprof: UprofRun,
    error: Option<String>,
}

impl BenchmarkParser {
    /// Reads one stdout line. Lines it doesn't recognise are Factorio's log and are skipped;
    /// the first malformed line it does recognise is kept as the run's error.
    pub fn line(&mut self, line: &str) {
        if self.error.is_some() {
            return;
        }
        if let Err(error) = self.read(line) {
            self.error = Some(error);
        }
    }

    fn read(&mut self, line: &str) -> Result<(), String> {
        if line.contains("heap stats:") {
            self.heap_stats.clear();
        }
        if (!self.heap_stats.is_empty() || line.contains("heap stats:"))
            && self.heap_stats.len() < mimalloc::STATS_LINES
        {
            self.heap_stats.push(line.to_string());
        }
        if self.uprof.read(line) {
            return Ok(());
        }
        let trimmed = line.trim();
        if self.version.is_none()
            && let Some(rest) = line.split("; Factorio ").nth(1)
        {
            self.version = rest.split_whitespace().next().map(String::from);
        } else if let Some(rest) = trimmed.strip_prefix("Performed ") {
            let (ticks, time) = rest
                .split_once(" updates in ")
                .ok_or_else(|| format!("unexpected line {line:?}"))?;
            self.performed = Some((number(ticks)?, number(time.trim_end_matches(" ms"))?));
        } else if let Some(rest) = trimmed.strip_prefix("avg: ") {
            let parts: Vec<&str> = rest.split(", ").collect();
            let [avg, min, max] = parts.as_slice() else {
                return Err(format!("unexpected line {line:?}"));
            };
            let field = |part: &str, name: &str| {
                number::<f64>(part.trim_start_matches(name).trim_end_matches(" ms"))
            };
            self.tick_times = Some(TickTimes {
                avg_ms: field(avg, "")?,
                min_ms: field(min, "min: ")?,
                max_ms: field(max, "max: ")?,
            });
        } else if let Some(rest) = trimmed.strip_prefix("tick,") {
            let columns = cells(rest).map(String::from).collect();
            self.verbose = Some(VerboseTimings {
                columns,
                rows: Vec::new(),
            });
        } else if let Some(verbose) = &mut self.verbose
            && let Some(rest) = trimmed.strip_prefix('t')
            && rest.starts_with(|first: char| first.is_ascii_digit())
        {
            let mut cells = cells(rest);
            let tick = number(cells.next().unwrap_or_default())?;
            let values = cells.map(number).collect::<Result<Vec<u64>, _>>()?;
            verbose.rows.push(VerboseRow { tick, values });
        }
        Ok(())
    }

    /// The run's measurement, or why the output didn't hold one: a malformed line, or a missing
    /// version, `Performed` or `avg:` line.
    pub fn finish(self) -> Result<Measurement, String> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let (ticks, execution_time_ms) = self.performed.ok_or("no \"Performed … updates\" line")?;
        let TickTimes {
            avg_ms,
            min_ms,
            max_ms,
        } = self.tick_times.ok_or("no \"avg: …\" line")?;
        let factorio_version = self.version.ok_or("no Factorio version line")?;
        Ok(Measurement {
            execution_time_ms,
            avg_ms,
            min_ms,
            max_ms,
            ticks,
            factorio_version,
            verbose: self.verbose,
            mimalloc: mimalloc::parse(&(self.heap_stats.join("\n") + "\n")),
            uprof: if self.uprof.is_empty() {
                None
            } else {
                Some(self.uprof)
            },
        })
    }
}

/// The cells of a verbose line, without the empty one after its trailing comma.
fn cells(rest: &str) -> impl Iterator<Item = &str> {
    rest.strip_suffix(',').unwrap_or(rest).split(',')
}

fn number<T: std::str::FromStr>(text: &str) -> Result<T, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{text:?} is not a number"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(output: &str) -> Result<Measurement, String> {
        let mut parser = BenchmarkParser::default();
        output.lines().for_each(|line| parser.line(line));
        parser.finish()
    }

    const PLAIN: &str =
        "   0.000 2026-11-24 20:50:18; Factorio 2.1.25 (build 88022, linux64, headless, space-age)
   0.520 Loading map /x/plain.zip: 753491 bytes.
  Performed 50 updates in 3.563 ms
  avg: 0.071 ms, min: 0.047 ms, max: 0.199 ms
  checksum: 1178193217
   0.615 Goodbye
";

    #[test]
    fn reads_execution_time_and_tick_times_from_factorio_output() {
        let measurement = parse(PLAIN).unwrap();

        let expected = Measurement {
            execution_time_ms: 3.563,
            avg_ms: 0.071,
            min_ms: 0.047,
            max_ms: 0.199,
            ticks: 50,
            factorio_version: "2.1.25".into(),
            verbose: None,
            mimalloc: None,
            uprof: None,
        };
        assert_eq!(measurement, expected);
    }

    #[test]
    fn reads_the_factorio_version_from_factorio_output() {
        assert_eq!(parse(PLAIN).unwrap().factorio_version, "2.1.25");
    }

    #[test]
    fn verbose_rows_are_read_in_factorio_column_order() {
        let output =
            format!("{PLAIN}run 1:\ntick,timestamp,wholeUpdate,\nt0,100,199375,\nt1,200,68639,\n");

        let verbose = parse(&output).unwrap().verbose.unwrap();

        let expected = VerboseTimings {
            columns: vec!["timestamp".into(), "wholeUpdate".into()],
            rows: vec![
                VerboseRow {
                    tick: 0,
                    values: vec![100, 199375],
                },
                VerboseRow {
                    tick: 1,
                    values: vec![200, 68639],
                },
            ],
        };
        assert_eq!(verbose, expected);
    }

    #[test]
    fn output_without_a_performed_line_is_an_error() {
        let output = "   0.000 2026-11-24 20:50:18; Factorio 2.1.25 (build 88022, linux64)\n";

        assert!(parse(output).is_err());
    }
}
