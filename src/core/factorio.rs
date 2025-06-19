use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::core::{BenchmarkError, Result};

use super::platform;

pub struct FactorioExecutor {
    executable_path: PathBuf,
}

impl FactorioExecutor {
    pub fn new(executable_path: PathBuf) -> Self {
        Self { executable_path }
    }

    pub fn discover(explicit_path: Option<PathBuf>) -> Result<Self> {
        let path = Self::find_executable(explicit_path)?;
        Ok(Self::new(path))
    }

    pub fn find_executable(explicit_path: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(path) = explicit_path {
            if path.exists() {
                tracing::info!("Using explicit Factorio path: {}", path.display());
                return Ok(path);
            } else {
                return Err(BenchmarkError::FactorioNotFoundAtPath { path: path });
            }
        }

        let candidates = platform::get_default_factorio_paths();

        for candidate in candidates {
            if candidate.exists() {
                tracing::info!("Found Factorio at: {}", candidate.display());
                return Ok(candidate);
            }
        }

        Err(BenchmarkError::FactorioNotFound)
    }

    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    pub fn create_command(&self) -> Command {
        Command::new(&self.executable_path)
    }

    // pub async fn get_version(&self) -> Result<String> { }
}
