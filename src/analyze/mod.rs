pub mod charts;
pub mod parser;

use crate::{
    core::config::{AnalyzeConfig, GlobalConfig},
    Result,
};

pub async fn run(_global_config: GlobalConfig, analyze_config: AnalyzeConfig) -> Result<()> {
    charts::generate_charts(&analyze_config)?;
    Ok(())
}
