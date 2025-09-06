//! Shared output utilities for writing results (e.g., CSVs, reports).

use std::path::Path;

use crate::{benchmark::{parser::BenchmarkResult, runner::VerboseData}, Result};

// Re-export submodules
pub mod csv;
pub mod report;
pub use csv::CsvWriter;

// Simple data holder
#[derive(Debug)]
pub enum WriteData {
    BenchmarkResults(Vec<BenchmarkResult>),
    VerboseData(Vec<VerboseData>),
    ReportData() // TODO
}

pub trait ResultWriter {
    fn write(&self, data: &WriteData, path: &Path) -> Result<()>;
}

pub fn ensure_output_dir(path: &Path) -> Result<()> { 
    std::fs::create_dir_all(path)?;
    Ok(())
}