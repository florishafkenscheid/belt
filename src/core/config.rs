//! Global configuration for BELT.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Global configuration for a BELT benchmarking session.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub factorio_path: Option<PathBuf>,
    pub verbose: bool,
}
