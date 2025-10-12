//! Error types for BELT.

use std::{
    fmt,
    num::{ParseFloatError, ParseIntError},
    path::PathBuf,
    string::FromUtf8Error,
};
use thiserror::Error;

/// The wrapper for the error kind, with an optional hint.
#[derive(Debug)]
pub struct BenchmarkError {
    kind: BenchmarkErrorKind,
    hint: Option<String>,
}

/// All types of errors than can occur in BELT.
#[derive(Error, Debug)]
pub enum BenchmarkErrorKind {
    #[error("Factorio executable not found. Please provide it explicitly with --factorio-path")]
    FactorioNotFound,

    #[error("Factorio executable not fund at provided path: {path}")]
    FactorioNotFoundAtPath { path: PathBuf },

    #[error("Save directory does not exist: {path}")]
    SaveDirectoryNotFound { path: PathBuf },

    #[error("No save files found matching pattern '{pattern}' in {directory}")]
    NoSaveFilesFound { pattern: String, directory: PathBuf },

    #[error("Invalid save file: {path} - {reason}")]
    InvalidSaveFile { path: PathBuf, reason: String },

    #[error("Invalid save file name: {path}")]
    InvalidSaveFileName { path: PathBuf },

    #[error("Invalid mods file name: {path}")]
    InvalidModsFileName { path: PathBuf },

    #[error("Invalid UTF-8 in Factorio output")]
    InvalidUtf8Output(#[from] FromUtf8Error),

    #[error("Progress bar template error: {0}")]
    ProgressBarError(#[from] indicatif::style::TemplateError),

    #[error("Factorio process failed with exit code {code}.")]
    FactorioProcessFailed { code: i32 },

    #[error("No benchmark results found in Factorio output")]
    NoBenchmarkResults,

    #[error("Failed to parse benchmark output: {reason}")]
    ParseError { reason: String },

    #[error("Template render error: {0}")]
    TemplateRenderError(#[from] handlebars::RenderError),

    #[error("Template error: {0}")]
    TemplateError(#[from] handlebars::TemplateError),

    #[error("CSV error: {0}")]
    CsvError(#[from] csv::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Glob error: {0}")]
    GlobError(#[from] glob::GlobError),

    #[error("Glob pattern error: {0}")]
    GlobPatternError(#[from] glob::PatternError),

    #[error("JSON Serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Chart generation error: {0}")]
    ChartGenerationError(#[from] charming::EchartsError),

    #[error("Invalid run order: {input}. Valid options: sequential, random, grouped")]
    InvalidRunOrder { input: String },

    #[error("Invalid WriteData")]
    InvalidWriteData,

    #[error("Belt-Sanitizer directory not found")]
    SanitizerNotFound,

    #[error("Data directory not found at: {path}")]
    DataDirectoryNotFound { path: PathBuf },

    #[error("No data files found at: {path}")]
    NoDataFilesFound { path: PathBuf },

    #[error("Expected data file not found at: {path}")]
    DataFileNotFound { path: PathBuf },

    #[error("Expected verbose data. None found.")]
    NoVerboseData,

    #[error("Invalid metric: {metric}")]
    InvalidMetric { metric: String },

    #[error("Couldn't parse into int: {0}")]
    ParseIntError(#[from] ParseIntError),

    #[error("Couldn't parse into float: {0}")]
    ParseFloatError(#[from] ParseFloatError),

    #[error("Tick mismatch, expected: {ticks}, got: {run_ticks}")]
    TickMismatch { ticks: usize, run_ticks: usize },

    #[error("No production statistics found")]
    NoProductionStatistics,

    #[error("No input statistics in production statistics found")]
    NoInputStatistics,

    #[error("No output statistics in production statistics found")]
    NoOutputStatistics,

    #[error("Blueprint directory does not exist: {path}")]
    BlueprintDirectoryNotFound { path: PathBuf },

    #[error("No blueprint files found matching pattern '{pattern}' in {directory}")]
    NoBlueprintFilesFound { pattern: String, directory: PathBuf },
}

/// Get a hint for the FactorioProcessFailed error, if it exists
impl BenchmarkError {
    /// Attaches a hint to the error
    pub fn with_hint(mut self, hint: Option<impl Into<String>>) -> Self {
        if let Some(hint) = hint {
            self.hint = Some(hint.into());
        }
        self
    }
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)?;
        if let Some(hint_text) = &self.hint {
            write!(f, " ({hint_text})")?;
        }

        Ok(())
    }
}

/// Proper error type
impl std::error::Error for BenchmarkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.kind)
    }
}

/// Convert Error into BenchmarkErrorKind
impl<E> From<E> for BenchmarkError
where
    BenchmarkErrorKind: From<E>,
{
    fn from(error: E) -> Self {
        BenchmarkError {
            kind: BenchmarkErrorKind::from(error),
            hint: None,
        }
    }
}

/// A convenient result type for BELT
pub type Result<T> = std::result::Result<T, BenchmarkError>;
