//! Configuration structs for BELT.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::RunOrder;

/// Global configuration for a BELT benchmarking session.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub factorio_path: Option<PathBuf>,
    pub verbose: bool,
}

/// Analyzation specific configuration
#[derive(Debug, Clone, Default)]
pub struct AnalyzeConfig {
    pub data_dir: PathBuf,
    pub smooth_window: u32,
    pub verbose_metrics: Vec<String>,
    pub height: u32,
    pub width: u32,
    pub max_points: Option<usize>,
}

/// Benchmarking specific configuration
#[derive(Debug, Clone, Default)]
pub struct BenchmarkConfig {
    pub saves_dir: PathBuf,
    pub ticks: u32,
    pub runs: u32,
    pub pattern: Option<String>,
    pub output: Option<PathBuf>,
    pub template_path: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
    pub run_order: RunOrder,
    pub verbose_metrics: Vec<String>,
    pub strip_prefix: Option<String>,
    pub headless: Option<bool>,
}

/// Sanitization specific configuration
#[derive(Debug, Clone)]
pub struct SanitizeConfig {
    pub saves_dir: PathBuf,
    pub pattern: Option<String>,
    pub ticks: u32,
    pub mods_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub items: Option<String>,
    pub fluids: Option<String>,
    pub headless: Option<bool>,
}
