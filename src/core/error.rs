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
    kind: Box<BenchmarkErrorKind>,
    hint: Option<String>,
    process_output: Option<String>,
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

    #[error("Invalid run order: {input}. Valid options: sequential, random, grouped")]
    InvalidRunOrder { input: String },

    #[error("Invalid WriteData")]
    InvalidWriteData,

    #[error("Belt-Sanitizer directory not found")]
    SanitizerNotFound,

    #[error("Couldn't parse into int: {0}")]
    ParseIntError(#[from] ParseIntError),

    #[error("Couldn't parse into float: {0}")]
    ParseFloatError(#[from] ParseFloatError),

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

    #[error("Invalid Blueprint file name: {path}")]
    InvalidBlueprintFileName { path: PathBuf },

    #[error("No mods directory found.")]
    NoModsDirectoryFound,

    #[error("Malformed benchmark output: {field} {string}")]
    MalformedBenchmarkOutput { field: String, string: String },

    #[error("Missing capture field: {field}")]
    MissingCaptureField { field: String },

    #[error("Failed to load configuration: {0}")]
    ConfigLoadError(String),

    #[error("Configuration file not found: {0}")]
    ConfigNotFound(PathBuf),
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

    /// Attaches captured child-process output to the error.
    pub fn with_process_output(mut self, stdout: &str, stderr: &str) -> Self {
        let output = format_process_output(stdout, stderr);
        if !output.is_empty() {
            self.process_output = Some(output);
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
        if let Some(output) = &self.process_output {
            write!(f, "\n\n{output}")?;
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
            kind: Box::new(BenchmarkErrorKind::from(error)),
            hint: None,
            process_output: None,
        }
    }
}

/// A convenient result type for BELT
pub type Result<T> = std::result::Result<T, BenchmarkError>;

fn format_process_output(stdout: &str, stderr: &str) -> String {
    let mut sections = Vec::new();

    if !stderr.trim().is_empty() {
        sections.push(format!("Factorio stderr:\n{}", tail_lines(stderr, 40)));
    }

    if !stdout.trim().is_empty() {
        sections.push(format!("Factorio stdout:\n{}", tail_lines(stdout, 40)));
    }

    sections.join("\n\n")
}

fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let skipped = lines.len().saturating_sub(max_lines);
    let mut output = String::new();

    if skipped > 0 {
        output.push_str(&format!("... omitted {skipped} earlier line(s)\n"));
    }

    output.push_str(&lines[skipped..].join("\n"));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_output_is_bounded() {
        let stderr = (0..45)
            .map(|i| format!("err {i}"))
            .collect::<Vec<_>>()
            .join("\n");

        let error = BenchmarkError::from(BenchmarkErrorKind::FactorioProcessFailed { code: 1 })
            .with_process_output("", &stderr)
            .to_string();

        assert!(error.contains("Factorio stderr:"));
        assert!(error.contains("omitted 5 earlier line(s)"));
        assert!(!error.contains("err 4\n"));
        assert!(error.contains("err 44"));
    }
}
