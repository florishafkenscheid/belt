//! Configuration management for BELT using Figment.
//!
//! This module provides hierarchical configuration support:
//! 1. CLI arguments (highest priority)
//! 2. Environment variables (BELT_*)
//! 3. Config file (~/.config/belt/config.toml)
//! 4. Default values (lowest priority)
//!
//! # Config File Location
//!
//! BELT looks for configuration in the following locations:
//! - `$BELT_CONFIG` environment variable (if set)
//! - `~/.config/belt/config.toml` (Linux/macOS)
//! - `%APPDATA%\belt\config.toml` (Windows)
//!
//! # Environment Variables
//!
//! Environment variables use double underscore (`__`) to separate the section from
//! the field name. This allows field names that contain underscores.
//!
//! Examples:
//! - `BELT_BENCHMARK__TICKS` → `benchmark.ticks`
//! - `BELT_BENCHMARK__RUNS` → `benchmark.runs`
//! - `BELT_ANALYZE__SMOOTH_WINDOW` → `analyze.smooth_window`
//! - `BELT_GLOBAL__VERBOSE` → `global.verbose`
//!
//! # Example Config File
//!
//! ```toml
//! [global]
//! factorio_path = "/opt/factorio/bin/factorio"
//! verbose = false
//!
//! [benchmark]
//! ticks = 6000
//! runs = 5
//! run_order = "grouped"
//! pattern = "*.zip"
//! headless = true
//!
//! [analyze]
//! smooth_window = 10
//! height = 800
//! width = 1200
//!
//! [sanitize]
//! ticks = 3600
//! headless = true
//!
//! [blueprint]
//! count = 10
//! buffer_ticks = 120
//! ```

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::core::RunOrder;

/// Default configuration file name
const CONFIG_FILENAME: &str = "config.toml";

/// Configuration directory name for BELT
const APP_NAME: &str = "belt";

// =============================================================================
// Error Handling
// =============================================================================

/// Errors that can occur during configuration loading
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to load configuration: {0}")]
    LoadError(String),
    #[error("Configuration file not found: {0}")]
    NotFound(PathBuf),
}

// =============================================================================
// Configuration Structs
// =============================================================================

/// Global configuration for a BELT benchmarking session.
///
/// This struct holds settings that apply across all subcommands.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Path to the Factorio executable
    pub factorio_path: Option<PathBuf>,
    /// Enable verbose logging output
    #[serde(default)]
    pub verbose: bool,
}

impl GlobalConfig {
    /// Load global configuration from figment
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract_inner("global")
            .or_else(|_| figment.extract())
            .map_err(|e| ConfigError::LoadError(e.to_string()))
    }
}

/// Analyzation specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeConfig {
    /// Directory containing benchmark data files
    #[serde(default)]
    pub data_dir: PathBuf,
    /// Window size for simple moving average smoothing (0 = no smoothing)
    #[serde(default)]
    pub smooth_window: u32,
    /// Metrics to generate per-tick charts for
    #[serde(default)]
    pub verbose_metrics: Vec<String>,
    /// Chart height in pixels
    #[serde(default = "default_height")]
    pub height: u32,
    /// Chart width in pixels
    #[serde(default = "default_width")]
    pub width: u32,
    /// Maximum data points before downsampling
    #[serde(default)]
    pub max_points: Option<usize>,
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::new(),
            smooth_window: 0,
            verbose_metrics: Vec::new(),
            height: default_height(),
            width: default_width(),
            max_points: None,
        }
    }
}

fn default_height() -> u32 {
    800
}

fn default_width() -> u32 {
    1200
}

impl AnalyzeConfig {
    /// Load configuration from figment
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract_inner("analyze")
            .or_else(|_| figment.extract())
            .map_err(|e| ConfigError::LoadError(e.to_string()))
    }
}

/// Benchmarking specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Directory containing save files to benchmark
    #[serde(default)]
    pub saves_dir: PathBuf,
    /// Number of ticks to run each benchmark
    #[serde(default = "default_ticks")]
    pub ticks: u32,
    /// Number of benchmark runs per save file
    #[serde(default = "default_runs")]
    pub runs: u32,
    /// Optional pattern to filter save files
    #[serde(default)]
    pub pattern: Option<String>,
    /// Output directory or file path
    #[serde(default)]
    pub output: Option<PathBuf>,
    /// Path to HTML report template
    #[serde(default)]
    pub template_path: Option<PathBuf>,
    /// Directory containing mods to use
    #[serde(default)]
    pub mods_dir: Option<PathBuf>,
    /// Execution order for benchmark runs
    #[serde(default)]
    pub run_order: RunOrder,
    /// Metrics to generate verbose charts for
    #[serde(default)]
    pub verbose_metrics: Vec<String>,
    /// Prefix to strip from save file names in output
    #[serde(default)]
    pub strip_prefix: Option<String>,
    /// Run Factorio in headless mode
    #[serde(default)]
    pub headless: Option<bool>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            saves_dir: PathBuf::new(),
            ticks: default_ticks(),
            runs: default_runs(),
            pattern: None,
            output: None,
            template_path: None,
            mods_dir: None,
            run_order: RunOrder::default(),
            verbose_metrics: Vec::new(),
            strip_prefix: None,
            headless: None,
        }
    }
}

fn default_ticks() -> u32 {
    6000
}

fn default_runs() -> u32 {
    5
}

impl BenchmarkConfig {
    /// Load configuration from figment
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract_inner("benchmark")
            .or_else(|_| figment.extract())
            .map_err(|e| ConfigError::LoadError(e.to_string()))
    }
}

/// Sanitization specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizeConfig {
    /// Directory containing save files to sanitize
    #[serde(default)]
    pub saves_dir: PathBuf,
    /// Optional pattern to filter save files
    #[serde(default)]
    pub pattern: Option<String>,
    /// Number of ticks to run sanitization
    #[serde(default = "default_sanitize_ticks")]
    pub ticks: u32,
    /// Directory containing mods to use
    #[serde(default)]
    pub mods_dir: Option<PathBuf>,
    /// Output directory for sanitized saves
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    /// Items to preserve during sanitization (comma-separated)
    #[serde(default)]
    pub items: Option<String>,
    /// Fluids to preserve during sanitization (comma-separated)
    #[serde(default)]
    pub fluids: Option<String>,
    /// Run Factorio in headless mode
    #[serde(default)]
    pub headless: Option<bool>,
}

fn default_sanitize_ticks() -> u32 {
    3600
}

impl Default for SanitizeConfig {
    fn default() -> Self {
        Self {
            saves_dir: PathBuf::new(),
            pattern: None,
            ticks: default_sanitize_ticks(),
            mods_dir: None,
            data_dir: None,
            items: None,
            fluids: None,
            headless: None,
        }
    }
}

impl SanitizeConfig {
    /// Load configuration from figment
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract_inner("sanitize")
            .or_else(|_| figment.extract())
            .map_err(|e| ConfigError::LoadError(e.to_string()))
    }
}

/// Blueprint Benchmarking specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintConfig {
    /// Directory containing blueprint files
    #[serde(default)]
    pub blueprints_dir: PathBuf,
    /// Path to the base save file for blueprint testing
    #[serde(default)]
    pub base_save_path: PathBuf,
    /// Number of blueprints to test
    #[serde(default)]
    pub count: u32,
    /// Number of buffer ticks before measuring
    #[serde(default)]
    pub buffer_ticks: u32,
    /// Directory containing mods to use
    #[serde(default)]
    pub mods_dir: Option<PathBuf>,
    /// Optional pattern to filter blueprint files
    #[serde(default)]
    pub pattern: Option<String>,
    /// Output directory or file path
    #[serde(default)]
    pub output: Option<PathBuf>,
    /// Prefix for output file names
    #[serde(default)]
    pub prefix: Option<String>,
    /// Run Factorio in headless mode
    #[serde(default)]
    pub headless: Option<bool>,
    /// Number of construction bots to use
    #[serde(default)]
    pub bot_count: Option<u32>,
}

impl Default for BlueprintConfig {
    fn default() -> Self {
        Self {
            blueprints_dir: PathBuf::new(),
            base_save_path: PathBuf::new(),
            count: 0,
            buffer_ticks: 0,
            mods_dir: None,
            pattern: None,
            output: None,
            prefix: None,
            headless: None,
            bot_count: None,
        }
    }
}

impl BlueprintConfig {
    /// Load configuration from figment
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        figment
            .extract_inner("blueprint")
            .or_else(|_| figment.extract())
            .map_err(|e| ConfigError::LoadError(e.to_string()))
    }
}

// Figment Configuration
// =============================================================================

/// Get the path to the configuration directory
fn get_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(APP_NAME))
}

/// Get the path to the configuration file
fn get_config_file_path() -> Option<PathBuf> {
    // Check for BELT_CONFIG environment variable first
    if let Ok(config_path) = std::env::var("BELT_CONFIG") {
        return Some(PathBuf::from(config_path));
    }
    // Otherwise use the standard config directory
    get_config_dir().map(|dir| dir.join(CONFIG_FILENAME))
}

/// Create a Figment with all configuration sources
///
/// Priority (highest to lowest):
/// 1. Environment variables (BELT_*)
/// 2. Config file
/// 3. Default values
pub fn create_figment() -> Result<Figment, ConfigError> {
    let mut figment = Figment::new();

    // Add config file if it exists
    if let Some(config_path) = get_config_file_path()
        && config_path.exists()
    {
        figment = figment.merge(Toml::file(config_path));
    }

    // Add environment variables with BELT_ prefix
    // Environment variables are mapped like: BELT_BENCHMARK__TICKS -> benchmark.ticks
    // Note: Use double underscore (__) to separate sections from field names
    figment = figment.merge(Env::prefixed("BELT_").split("__"));

    Ok(figment)
}

/// Create a Figment from a specific config file path
pub fn create_figment_from_file(path: &PathBuf) -> Result<Figment, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.clone()));
    }

    let figment = Figment::new()
        .merge(Toml::file(path))
        .merge(Env::prefixed("BELT_").split("__"));

    Ok(figment)
}

/// Initialize the configuration directory with an example config file
pub fn init_config_dir() -> Result<PathBuf, ConfigError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| ConfigError::LoadError("Could not find config directory".to_string()))?;
    let belt_config_dir = config_dir.join(APP_NAME);
    let config_file = belt_config_dir.join(CONFIG_FILENAME);

    // Create directory if it doesn't exist
    if !belt_config_dir.exists() {
        std::fs::create_dir_all(&belt_config_dir)
            .map_err(|e| ConfigError::LoadError(e.to_string()))?;
    }

    // Create example config if it doesn't exist
    if !config_file.exists() {
        let example_config = r#"# BELT Configuration File
# Place this file at ~/.config/belt/config.toml (Linux/macOS)
# or %APPDATA%\belt\config.toml (Windows)
# Or set BELT_CONFIG environment variable to point to your config file

[global]
# Path to Factorio executable
# factorio_path = "/opt/factorio/bin/factorio"
# verbose = false

[benchmark]
# ticks = 6000
# runs = 5
# run_order = "grouped"  # Options: "sequential", "random", "grouped"
# pattern = "*.zip"
# headless = true

[analyze]
# smooth_window = 0
# height = 800
# width = 1200

[sanitize]
# ticks = 3600
# headless = true

[blueprint]
# count = 10
# buffer_ticks = 120
"#;
        std::fs::write(&config_file, example_config)
            .map_err(|e| ConfigError::LoadError(e.to_string()))?;
    }

    Ok(config_file)
}
