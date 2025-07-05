//! Error types for BELT.

use std::path::PathBuf;
use thiserror::Error;

/// All errors than can occur in BELT.
#[derive(Error, Debug)]
pub enum BenchmarkError {
    #[error("Factorio executable not found")]
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
    InvalidUtf8Output,

    #[error("Progress bar template error: {0}")]
    ProgressBarError(String),

    #[error("Factorio process failed with exit code {code}.")]
    FactorioProcessFailed { code: i32, hint: Option<String> },

    #[error("No benchmark results found in Factorio output")]
    NoBenchmarkResults,

    #[error("Failed to parse benchmark output: {reason}")]
    ParseError { reason: String },

    #[error("Template error: {0}")]
    TemplateError(#[from] handlebars::RenderError),

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

    #[error("Failed to create directory: {path}")]
    DirectoryCreationFailed { path: PathBuf },

    #[error("Invalid run order: {input}. Valid options: sequential, random, grouped")]
    InvalidRunOrder { input: String },

    #[error("Invalid blueprint path: {path} - {reason}")]
    InvalidBlueprintPath { path: PathBuf, reason: String },

    #[error("Invalid blueprint string: {path} - {reason}")]
    InvalidBlueprintString { path: PathBuf, reason: String },

    #[error("Blueprint decoding error: {path} - {reason}")]
    BlueprintDecode { path: PathBuf, reason: String },

    #[error("Blueprint encoding error: {reason}")]
    BlueprintEncode { reason: String },
}

/// Get a hint for the FactorioProcessFailed error, if it exists
impl BenchmarkError {
    pub fn get_hint(&self) -> Option<&str> {
        if let BenchmarkError::FactorioProcessFailed { hint, .. } = self {
            hint.as_deref()
        } else {
            None
        }
    }
}

/// A convenient result type for BELT
pub type Result<T> = std::result::Result<T, BenchmarkError>;
