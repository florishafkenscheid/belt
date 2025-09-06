use std::{collections::HashMap, path::PathBuf};

use crate::{
    benchmark::runner::VerboseData,
    core::{error::{BenchmarkErrorKind, Result}, utils},
};

/// Scan directory for csv files and generate charts
pub fn generate_charts(data_dir: &PathBuf) -> Result<()> {
    // Scan directory, look for glob
    let (results, verbose_data_by_save) = if data_dir.is_dir() {
        // Read those files
        // Make sure:
        // 1. *.csv,
        // 2. results.csv,
        // 3. *_verbose_metrics.csv
        let files = utils::find_data_files(data_dir)?;
        
        for file in files {
            
        }
        
        
        
        
        let res = "hi".to_string();
        let verb: HashMap<String, Vec<VerboseData>> = HashMap::new();
        (res, verb)
    } else {
        return Err(BenchmarkErrorKind::DataDirectoryNotFound {
            path: data_dir.to_path_buf(),
        }
        .into());
    };

    if results.is_empty() {
        return Err(BenchmarkErrorKind::NoBenchmarkResults.into());
    }

    // Generate charts
    // Standard
    draw_ups_chart(results)?;
    draw_boxplot_chart(results)?;
    draw_improvement_chart(results)?;

    // Verbose
    draw_metric_chart(verbose_data_by_save)?;
    draw_line_chart(verbose_data_by_save)?;
    draw_min_chart(verbose_data_by_save)?;

    Ok(())
}

fn draw_ups_chart() -> Result<()> {
    Ok(())
}

fn draw_boxplot_chart() -> Result<()> {
    Ok(())
}

fn draw_improvement_chart() -> Result<()> {
    Ok(())
}

fn draw_metric_chart(data: HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}

fn draw_line_chart(data: HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}

fn draw_min_chart(data: HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}
