pub mod charts;
pub mod parser;

use crate::{
    Result,
    core::config::{AnalyzeConfig, GlobalConfig},
};

pub async fn run(_global_config: GlobalConfig, analyze_config: AnalyzeConfig) -> Result<()> {
    charts::generate_charts(&analyze_config)?;
    Ok(())
}
