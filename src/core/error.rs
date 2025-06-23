use std::path::PathBuf;
use thiserror::Error;

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

    #[error("Factorio process failed with exit code {code}: {err}")]
    FactorioProcessFailed { code: i32, err: String },

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
}

pub type Result<T> = std::result::Result<T, BenchmarkError>;
