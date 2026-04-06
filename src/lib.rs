//! Library root for BELT.
//!
//! Exposes core benchmarking and configuration APIs.

pub mod benchmark;
pub mod core;
pub mod sanitize;

/// Re-export commonly used types for convenience.
pub use core::config::{BenchmarkConfig, GlobalConfig};
pub use core::error::{BenchmarkError, BenchmarkErrorKind, Result};
