use crate::{
    benchmark::parser::BenchmarkResult,
    core::{BenchmarkError, Result},
};
use std::{collections::HashMap, path::Path};

use charming::{
    Chart, ImageRenderer,
    component::{Axis, Grid, Title},
    element::{AxisType, DimensionEncode, Label, LabelPosition, SplitArea, SplitLine},
    series::{Bar, Boxplot, Scatter},
    theme::Theme,
};

pub fn generate_charts(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    if results.is_empty() {
        return Err(BenchmarkError::NoBenchmarkResults);
    }

    let mut renderer = ImageRenderer::new(1000, 1000).theme(Theme::Walden);

    let ups_charts = generate_ups_charts(results)?; // Returns Vec<Chart>
    let base_chart = generate_base_chart(results)?; // Returns Chart

    let mut charts = Vec::new();
    charts.extend(ups_charts); // So, have to extend & push
    charts.push(base_chart);

    for (index, chart) in charts.iter().enumerate() {
        renderer.save(
            chart,
            output_dir.join(format!("result_{}_chart.svg", index)),
        )?;
    }

    Ok(())
}

fn generate_ups_charts(results: &[BenchmarkResult]) -> Result<Vec<Chart>> {
    let mut charts = Vec::new();

    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    let avg_ups_values: Vec<i64> = results
        .iter()
        .map(|result| {
            let total_ups: f64 = result.runs.iter().map(|run| run.effective_ups).sum();
            (total_ups / result.runs.len() as f64).round() as i64
        })
        .collect();

    // Bar chart
    let bar_chart = Chart::new()
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
        .y_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(save_names.clone()),
        )
        .series(
            Bar::new()
                .name("Effective UPS")
                .data(avg_ups_values)
                .label(Label::new().show(true).position(LabelPosition::Inside)),
        );
    charts.push(bar_chart);

    // Box plot chart
    let save_name_to_index: HashMap<String, usize> = save_names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect();

    let mut grouped_boxplot_data: Vec<Vec<f64>> = vec![Vec::new(); save_names.len()];
    let mut all_individual_ups: Vec<f64> = Vec::new();

    for result in results {
        let category_index = *save_name_to_index.get(&result.save_name).ok_or_else(|| {
            BenchmarkError::ParseError {
                reason: format!("Save name {} not found in category map", result.save_name),
            }
        })?;

        for run in &result.runs {
            grouped_boxplot_data[category_index].push(run.effective_ups);
            all_individual_ups.push(run.effective_ups);
        }
    }

    let min_ups = all_individual_ups
        .iter()
        .cloned()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    let max_ups = all_individual_ups
        .iter()
        .cloned()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    // Add a small buffer for better visualization.
    let y_axis_min_buffered = (min_ups * 0.95).floor();
    let y_axis_max_buffered = (max_ups * 1.05).ceil();

    tracing::debug!(
        "Min UPS: {}, Max UPS: {}, Boxplot Data: {:?}",
        min_ups,
        max_ups,
        grouped_boxplot_data
    );

    let boxplot_chart = Chart::new()
        .title(Title::new().text("Benchmark Results - Effective UPS Distribution"))
        .grid(Grid::new().left("10%").right("10%").bottom("15%"))
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(save_names)
                .boundary_gap(true)
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(false)),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .name("UPS")
                .min(y_axis_min_buffered)
                .max(y_axis_max_buffered)
                .split_area(SplitArea::new().show(true)),
        )
        .series(Boxplot::new().name("boxplot").data(grouped_boxplot_data))
        .series(
            Scatter::new()
                .name("outlier")
                .encode(DimensionEncode::new().x(1).y(0)),
        );
    charts.push(boxplot_chart);

    Ok(charts)
}

fn generate_base_chart(results: &[BenchmarkResult]) -> Result<Chart> {
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
