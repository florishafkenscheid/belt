//! Library root for BELT.
//!
//! Exposes core benchmarking and configuration APIs.

pub mod benchmark;
pub mod core;

/// Re-export commonly used types for convenience.
pub use benchmark::BenchmarkConfig;
pub use core::GlobalConfig;
pub use core::error::{BenchmarkError, Result};
