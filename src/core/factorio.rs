//! The wrapper for the Factorio binary.

use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::core::{Result, error::BenchmarkErrorKind};

use super::platform;

pub struct FactorioExecutor {
    executable_path: PathBuf,
}

impl FactorioExecutor {
    pub fn new(executable_path: PathBuf) -> Self {
        Self { executable_path }
    }

    /// Find the binary and create a FactorioExecutor with that path
    pub fn discover(explicit_path: Option<PathBuf>) -> Result<Self> {
        let path = Self::find_executable(explicit_path)?;
        Ok(Self::new(path))
    }

    /// Find the binary
    pub fn find_executable(explicit_path: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(path) = explicit_path {
            if path.exists() && path.is_file() {
                tracing::info!("Using explicit Factorio path: {}", path.display());
                return Ok(path);
            } else {
                return Err(BenchmarkErrorKind::FactorioNotFoundAtPath { path }.into());
            }
        }

        // Get possible locations of the binary, based on the user's operating system
        let candidates = platform::get_default_factorio_paths();

        // Check each candidate for if it exists
        for candidate in candidates {
            if candidate.exists() {
                tracing::info!("Found Factorio at: {}", candidate.display());
                return Ok(candidate);
            }
        }

        Err(BenchmarkErrorKind::FactorioNotFound.into())
    }

    /// Getter for the executable_path
    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    /// Public API for creating a command
    pub fn create_command(&self) -> Command {
        Command::new(&self.executable_path)
    }
}
