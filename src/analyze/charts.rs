use std::{collections::HashMap, path::PathBuf};
use plotters::prelude::*;

use crate::{
    analyze::parser,
    benchmark::{parser::BenchmarkResult, runner::VerboseData},
    core::error::{BenchmarkErrorKind, Result},
};

/// Scan directory for csv files and generate charts
pub fn generate_charts(data_dir: &PathBuf) -> Result<()> {
    // Scan directory, look for glob
    let (results, verbose_data_by_save) = if data_dir.is_dir() {
        parser::read_data(data_dir)?
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
    draw_ups_chart(&results)?;
    draw_boxplot_chart(&results)?;
    draw_improvement_chart(&results)?;

    // Verbose
    draw_metric_chart(&verbose_data_by_save)?;
    draw_line_chart(&verbose_data_by_save)?;
    draw_min_chart(&verbose_data_by_save)?;

    Ok(())
}

fn draw_ups_chart(data: &Vec<BenchmarkResult>) -> Result<()> {
    Ok(())
}

fn draw_boxplot_chart(data: &Vec<BenchmarkResult>) -> Result<()> {
    Ok(())
}

fn draw_improvement_chart(data: &Vec<BenchmarkResult>) -> Result<()> {
    Ok(())
}

fn draw_metric_chart(data: &HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}

fn draw_line_chart(data: &HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}

fn draw_min_chart(data: &HashMap<String, Vec<VerboseData>>) -> Result<()> {
    Ok(())
}
