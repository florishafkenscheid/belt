use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub factorio_path: Option<PathBuf>,
    pub verbose: bool,
    pub output_dir: PathBuf,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            factorio_path: None,
            verbose: false,
            output_dir: PathBuf::from("./output"),
        }
    }
}
