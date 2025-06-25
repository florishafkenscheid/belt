use crate::core::{BenchmarkError, Result};
use std::path::Path;

use charming::{
    Chart, ImageRenderer,
    component::{Axis, Grid, Title},
    element::{AxisType, Label, LabelPosition},
    series::Bar,
    theme::Theme,
};

use crate::benchmark::parser::BenchmarkResult;

pub fn generate_charts(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    if results.is_empty() {
        return Err(BenchmarkError::NoBenchmarkResults);
    }

    let mut renderer = ImageRenderer::new(500, 400).theme(Theme::Walden);

    let ups_chart = generate_ups_chart(results)?;
    let base_chart = generate_base_chart(results)?;

    let charts = vec![ups_chart, base_chart];
    for (index, chart) in charts.iter().enumerate() {
        renderer.save(
            chart,
            output_dir.join(format!("result_{}_chart.svg", index)),
        )?;
    }

    Ok(())
}

pub fn generate_ups_chart(results: &[BenchmarkResult]) -> Result<Chart> {
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    let avg_ups_values: Vec<i64> = results
        .iter()
        .map(|result| {
            let total_ups: f64 = result.runs.iter().map(|run| run.effective_ups).sum();
            (total_ups / result.runs.len() as f64) as i64
        })
        .collect();

    let chart = Chart::new()
        .title(Title::new().text("Benchmark Results - Average Effective UPS"))
        .grid(
            Grid::new()
                .left("3%")
                .right("4%")
                .bottom("3%")
                .contain_label(true),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Value)
                .boundary_gap(("0", "0.01")),
        )
        .y_axis(Axis::new().type_(AxisType::Category).data(save_names))
        .series(
            Bar::new()
                .name("Effective UPS")
                .data(avg_ups_values)
                .label(Label::new().show(true).position(LabelPosition::Inside)),
        );

    Ok(chart)
}

pub fn generate_base_chart(results: &[BenchmarkResult]) -> Result<Chart> {
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    let base_diffs: Vec<f64> = results
        .iter()
        .map(|result| {
            let total_base_diffs: f64 = result.runs.iter().map(|run| run.base_diff).sum();
            let avg = total_base_diffs / result.runs.len() as f64;
            (avg * 100.0).round() / 100.0
        })
        .collect();

    let chart = Chart::new()
        .title(Title::new().text("Benchmark Results - Percentage Improvement"))
        .grid(
            Grid::new()
                .left("3%")
                .right("4%")
                .bottom("3%")
                .contain_label(true),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Value)
                .boundary_gap(("0", "0.01")),
        )
        .y_axis(Axis::new().type_(AxisType::Category).data(save_names))
        .series(
            Bar::new()
                .name("Percentage Improvement")
                .data(base_diffs)
                .label(Label::new().show(true).position(LabelPosition::Inside)),
        );

    Ok(chart)
}
