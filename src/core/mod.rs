//! Core functionality for BELT.
//!
//! Provides configuration, error types, Factorio process management, output handling, and platform utilities.

pub mod config;
pub mod error;
pub mod factorio;
pub mod output;
pub mod platform;
pub mod settings;
pub mod utils;

pub use config::GlobalConfig;
pub use error::Result;
pub use factorio::FactorioExecutor;
pub use utils::*;
