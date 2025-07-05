//! Chart generation for benchmark results.
//!
//! Uses the `charming` crate to render SVG charts for UPS and improvement metrics.

use crate::{
    benchmark::parser::BenchmarkResult,
    core::{BenchmarkError, Result},
};
use std::path::Path;

use charming::{
    Chart, ImageRenderer,
    component::{Axis, Grid, Title},
    element::{AxisLabel, AxisType, ItemStyle, Label, LabelPosition, SplitArea, SplitLine},
    series::{Bar, Boxplot, Line, Scatter},
    theme::Theme,
};

/// Generates all charts for the given benchmark results.
///
/// Returns an error fi no results are provided.
pub async fn generate_charts(results: &[BenchmarkResult], output_dir: &Path) -> Result<()> {
    if results.is_empty() {
        return Err(BenchmarkError::NoBenchmarkResults);
    }

    let ups_charts = generate_ups_charts(results)?; // Returns Vec<Chart>
    let base_chart = generate_base_chart(results)?; // Returns Chart

    let mut charts = Vec::new();
    charts.extend(ups_charts); // So, have to extend & push
    charts.push(base_chart);

    let mut renderer = ImageRenderer::new(1000, 1000).theme(Theme::Walden);
    // Write all charts to files
    for (index, chart) in charts.iter().enumerate() {
        renderer.save(chart, output_dir.join(format!("result_{index}_chart.svg")))?;
    }

    Ok(())
}

/// Generates ups charts for the given benchmark results.
fn generate_ups_charts(results: &[BenchmarkResult]) -> Result<Vec<Chart>> {
    let mut charts = Vec::new();

    // Collect save names
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    // Collect the average ups values
    let avg_ups_values: Vec<i64> = results
        .iter()
        .map(|result| {
            let total_ups: f64 = result.runs.iter().map(|run| run.effective_ups).sum();
            (total_ups / result.runs.len() as f64).round() as i64
        })
        .collect();

    // Bar chart
    let bar_chart = Chart::new()
        .title(
            Title::new()
                .text("Benchmark Results - Average Effective UPS")
                .left("center"),
        )
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
                .boundary_gap(("0", "0.01"))
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(false)),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(save_names.clone())
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(true)),
        )
        .series(
            Bar::new()
                .name("Effective UPS")
                .data(avg_ups_values)
                .label(Label::new().show(true).position(LabelPosition::Inside)),
        );
    charts.push(bar_chart);

    // Box plot chart
    let boxplot_data = calculate_boxplot_data(results);

    let y_axis_min_buffered = (boxplot_data.min_value * 0.95).floor();
    let y_axis_max_buffered = (boxplot_data.max_value * 1.05).ceil();

    let boxplot_chart = Chart::new()
        .title(
            Title::new()
                .text("Benchmark Results - Effective UPS Distribution")
                .left("center"),
        )
        .grid(
            Grid::new()
                .left("10%")
                .right("10%")
                .bottom("7.5%")
                .contain_label(true),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(boxplot_data.category_names)
                .boundary_gap(true)
                .axis_label(AxisLabel::new().rotate(45.0).interval(0))
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(true)),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .name("UPS")
                .min(y_axis_min_buffered)
                .max(y_axis_max_buffered)
                .interval((y_axis_max_buffered - y_axis_min_buffered) / 5.0)
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(false)),
        )
        .series(
            Boxplot::new()
                .name("boxplot")
                .data(boxplot_data.boxplot_values)
                .item_style(ItemStyle::new().border_width(1).border_color("#3FB1E3")),
        )
        .series(
            Scatter::new()
                .name("outlier")
                .data(boxplot_data.outlier_values)
                .symbol_size(10),
        );
    charts.push(boxplot_chart);

    Ok(charts)
}

/// Generate a line chart from verbose per-tick benchmark data
pub fn generate_verbose_chart(verbose_csv_data: &str, title: &str) -> Result<Chart> {
    let mut reader = csv::Reader::from_reader(verbose_csv_data.as_bytes());

    let mut ticks: Vec<u64> = Vec::new();
    let mut whole_updates_ms: Vec<f64> = Vec::new();

    for result in reader.records() {
        let record = result?;
        if let (Some(tick_str), Some(update_ns_str)) = (record.get(0), record.get(2)) {
            if let Ok(tick) = tick_str.trim_start_matches('t').parse::<u64>() {
                if let Ok(update_ns) = update_ns_str.parse::<f64>() {
                    ticks.push(tick);
                    whole_updates_ms.push(update_ns / 1_000_000.0); // Convert to milliseconds for readability
                }
            }
        }
    }
    let tick_labels: Vec<String> = ticks.iter().map(|t| t.to_string()).collect();

    let chart = Chart::new()
        .title(Title::new().text(title).left("center"))
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(tick_labels)
                .split_line(SplitLine::new().show(false)),
        )
        .y_axis(Axis::new().type_(AxisType::Value).name("Update Time (ms)"))
        .series(Line::new().data(whole_updates_ms).show_symbol(false));

    Ok(chart)
}

/// Generate the improvement percentage chart for the given benchmark results
fn generate_base_chart(results: &[BenchmarkResult]) -> Result<Chart> {
    // Collect save names
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    // Collect base differences
    let base_diffs: Vec<f64> = results
        .iter()
        .map(|result| {
            let total_base_diffs: f64 = result.runs.iter().map(|run| run.base_diff).sum();
            let avg = total_base_diffs / result.runs.len() as f64;
            (avg * 100.0).round() / 100.0
        })
        .collect();

    // Create the chart
    let chart = Chart::new()
        .title(
            Title::new()
                .text("Benchmark Results - Percentage Improvement")
                .left("center"),
        )
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
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(false)),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(save_names)
                .split_area(SplitArea::new().show(false))
                .split_line(SplitLine::new().show(true)),
        )
        .series(
            Bar::new()
                .name("Percentage Improvement")
                .data(base_diffs)
                .label(Label::new().show(true).position(LabelPosition::Inside)),
        );

    Ok(chart)
}

struct BoxplotData {
    boxplot_values: Vec<Vec<f64>>,
    outlier_values: Vec<Vec<f64>>,
    category_names: Vec<String>,
    min_value: f64,
    max_value: f64,
}

/// Manually calculate the boxplot data given the benchmark results
fn calculate_boxplot_data(results: &[BenchmarkResult]) -> BoxplotData {
    // Collect save names
    let save_names: Vec<String> = results
        .iter()
        .map(|result| result.save_name.clone())
        .collect();

    let mut grouped_boxplot_data: Vec<Vec<f64>> = Vec::new();
    let mut outliers: Vec<(usize, f64)> = Vec::new();
    let mut all_individual_ups: Vec<f64> = Vec::new();

    // Iterate over every result and push UPS values
    for result in results {
        let mut values: Vec<f64> = result.runs.iter().map(|run| run.effective_ups).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        all_individual_ups.extend(&values);
        grouped_boxplot_data.push(values);
    }

    // Calculate boxplot statistics manually
    let mut boxplot_data: Vec<Vec<f64>> = Vec::new();

    for (category_idx, values) in grouped_boxplot_data.iter().enumerate() {
        if values.is_empty() {
            continue;
        };

        let len = values.len();
        let q1_idx = len / 4;
        let q2_idx = len / 2;
        let q3_idx = (3 * len) / 4;

        let q1 = values[q1_idx];
        let q2 = values[q2_idx]; // median
        let q3 = values[q3_idx];
        let iqr = q3 - q1;

        let lower_fence = q1 - 1.5 * iqr;
        let upper_fence = q3 + 1.5 * iqr;

        // Find whiskers (actual min/max within fences)
        let lower_whisker = values
            .iter()
            .find(|&&v| v >= lower_fence)
            .unwrap_or(&values[0]);
        let upper_whisker = values
            .iter()
            .rev()
            .find(|&&v| v <= upper_fence)
            .unwrap_or(&values[len - 1]);

        // Collect outliers
        for &value in values {
            if value < lower_fence || value > upper_fence {
                outliers.push((category_idx, value));
            }
        }

        // Boxplot data format: [min, Q1, median, Q3, max]
        boxplot_data.push(vec![*lower_whisker, q1, q2, q3, *upper_whisker]);
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

    // Convert outliers to scatter data
    let scatter_data: Vec<Vec<f64>> = outliers
        .into_iter()
        .map(|(category, value)| vec![category as f64, value])
        .collect();

    BoxplotData {
        boxplot_values: boxplot_data,
        outlier_values: scatter_data,
        category_names: save_names,
        min_value: min_ups,
        max_value: max_ups,
    }
}
