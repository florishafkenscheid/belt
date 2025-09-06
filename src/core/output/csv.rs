use std::path::Path;

use crate::{
    BenchmarkErrorKind, Result,
    benchmark::{parser::BenchmarkResult, runner::VerboseData},
    core::output::{ResultWriter, WriteData},
};

pub struct CsvWriter {}

impl CsvWriter {
    pub fn new() -> Self {
        Self {}
    }
}

impl ResultWriter for CsvWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()> {
        match data {
            WriteData::BenchmarkResults(results) => write_benchmark_csv(results, path),
            WriteData::VerboseData(verbose) => write_verbose_csv(verbose, path),
            _ => Err(BenchmarkErrorKind::FactorioNotFound.into()), // TODO
        }
    }
}

fn write_benchmark_csv(results: &[BenchmarkResult], path: &Path) -> Result<()> {
    Ok(())
}

fn write_verbose_csv(verbose: &[VerboseData], path: &Path) -> Result<()> {
    Ok(())
}
