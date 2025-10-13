//! Shared output utilities for writing results (e.g., CSVs, reports).

use std::path::{Path, PathBuf};

use crate::{
    benchmark::{parser::BenchmarkResult, runner::VerboseData},
    Result,
};

// Re-export submodules
pub mod csv;
pub mod report;
pub use csv::CsvWriter;

// Simple data holder
#[derive(Debug)]
pub enum WriteData {
    Benchmark(Vec<BenchmarkResult>),

    Verbose {
        data: Vec<VerboseData>,
        metrics_to_export: Vec<String>,
    },

    Report {
        data: Vec<BenchmarkResult>,
        template_path: Option<PathBuf>,
    },
}

pub trait ResultWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()>;
}

pub fn ensure_output_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}
