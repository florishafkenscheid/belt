//! Shared output utilities for writing results (e.g., CSVs, reports).

use std::path::Path;

use crate::{
    Result,
    benchmark::{parser::BenchmarkRun, runner::VerboseData},
};

// Re-export submodules
pub mod csv;
pub mod report;
mod uprof;
pub use csv::CsvWriter;

// Simple data holder
#[derive(Debug)]
pub enum WriteData<'a> {
    Benchmark(Vec<BenchmarkRun>),

    Verbose {
        data: Vec<VerboseData>,
        metrics_to_export: Vec<String>,
    },

    Report {
        data: Vec<BenchmarkRun>,
        template_path: Option<&'a Path>,
    },
}

pub trait ResultWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()>;
    fn append(&self, data: &WriteData, path: &Path) -> Result<()>;
}

pub fn ensure_output_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

pub fn write_result(
    writer: &impl ResultWriter,
    data: &WriteData,
    output_dir: &Path,
    append: bool,
) -> Result<()> {
    if append {
        writer.append(data, output_dir)
    } else {
        writer.write(data, output_dir)
    }
}
