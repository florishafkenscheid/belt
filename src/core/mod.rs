pub mod config;
pub mod error;
pub mod factorio;
pub mod output;
pub mod platform;

pub use config::GlobalConfig;
pub use error::{BenchmarkError, Result};
pub use factorio::FactorioExecutor;
