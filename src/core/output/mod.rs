//! Shared output utilities for writing results (e.g., CSVs, reports).

use crate::benchmark::{parser::BenchmarkResult, runner::VerboseData};

#[derive(Debug)]
pub enum WriteData {
    BenchmarkResults(Vec<BenchmarkResult>),
    VerboseData(Vec<VerboseData>),
    // ReportData()
}