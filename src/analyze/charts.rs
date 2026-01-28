//! Chart generation for benchmark results.
//!
//! Uses the `charming` crate to render SVG charts for UPS and improvement metrics.

use charming::{
    Chart, ImageRenderer,
    component::{Axis, Grid, Title},
    element::{
        AxisLabel, AxisType, ItemStyle, JsFunction, Label, LabelPosition, SplitArea, SplitLine,
    },
    series::{Bar, Boxplot, Line, Scatter},
    theme::Theme,
};

use crate::{
    analyze::parser,
    benchmark::{parser::BenchmarkResult, runner::VerboseData},
    core::{
        config::AnalyzeConfig,
        error::{BenchmarkErrorKind, Result},
        utils,
    },
};

/// Scan directory for csv files and generate charts
pub fn generate_charts(analyze_config: &AnalyzeConfig) -> Result<()> {
    let data_dir = &analyze_config.data_dir;
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
    let mut charts: Vec<(Chart, String)> = Vec::new();
    // Standard
    charts.push(draw_ups_chart(&results)?);
    charts.push(draw_boxplot_chart(&results)?);
    charts.push(draw_improvement_chart(&results)?);

    // Verbose
    for (save_name, data) in &verbose_data_by_save {
        let first_csv = &data[0].csv_data;
        let mut reader = csv::Reader::from_reader(first_csv.as_bytes());
        let headers: Vec<String> = reader.headers()?.iter().map(|s| s.to_string()).collect();
        let metrics_to_chart: Vec<String> =
            if analyze_config.verbose_metrics.contains(&"all".to_string()) {
                headers
                    .into_iter()
                    .filter(|h| h != "tick" && h != "timestamp" && !h.is_empty())
                    .collect()
            } else {
                analyze_config.verbose_metrics.clone()
            };

        for metric in metrics_to_chart {
            let prepped_data = prepare_metric(save_name, data, &metric, analyze_config)?;
            charts.push(draw_metric_chart(&prepped_data, &metric)?);
            charts.push(draw_min_chart(&prepped_data, &metric)?);
        }
    }

    let mut renderer =
        ImageRenderer::new(analyze_config.width, analyze_config.height).theme(Theme::Walden);
    for (chart, title) in charts {
        renderer.save(&chart, data_dir.join(format!("{title}.svg")))?;
    }

    tracing::info!("Analyzation complete!");

    Ok(())
}

fn draw_ups_chart(data: &[BenchmarkResult]) -> Result<(Chart, String)> {
    let save_names: Vec<String> = data.iter().map(|result| result.save_name.clone()).collect();

    let avg_ups_values: Vec<i64> = data
        .iter()
        .map(|result| {
            let total_ups: f64 = result.runs.iter().map(|run| run.effective_ups).sum();
            (total_ups / result.runs.len() as f64).round() as i64
        })
        .collect();

    Ok((
        Chart::new()
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
            ),
        "average_ups".to_string(),
    ))
}

fn draw_boxplot_chart(data: &[BenchmarkResult]) -> Result<(Chart, String)> {
    let boxplot_data = utils::calculate_boxplot_data(data);
    let y_min = (boxplot_data.min_value * 0.95).floor();
    let y_max = (boxplot_data.max_value * 1.05).ceil();

    Ok((
        Chart::new()
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
                    .min(y_min)
                    .max(y_max)
                    .interval((y_max - y_min) / 5.0)
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
            ),
        "boxplot".to_string(),
    ))
}

fn draw_improvement_chart(data: &[BenchmarkResult]) -> Result<(Chart, String)> {
    let save_names: Vec<String> = data.iter().map(|result| result.save_name.clone()).collect();

    let base_diffs: Vec<f64> = data
        .iter()
        .map(|result| {
            let total_base_diffs: f64 = result.runs.iter().map(|run| run.base_diff).sum();
            let avg = total_base_diffs / result.runs.len() as f64;
            (avg * 100.0).round() / 100.0
        })
        .collect();

    Ok((
        Chart::new()
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
            ),
        "improvement_percentage".to_string(),
    ))
}

fn draw_metric_chart(data: &PreppedVerboseData, metric: &String) -> Result<(Chart, String)> {
    let title = format!("{} per Tick for {}", metric, data.save_name);
    let y_axis_name = format!("{metric} Time (ms)");
    let tick_labels = data.ticks.iter().map(|t| t.to_string()).collect();

    let mut chart = Chart::new()
        .title(Title::new().text(title).left("center"))
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(tick_labels)
                .split_line(SplitLine::new().show(false)),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .name(y_axis_name)
                .min(data.y_min)
                .max(data.y_max)
                .axis_label(AxisLabel::new().formatter(JsFunction::new_with_args(
                    "value",
                    "return value.toFixed(3);",
                ))),
        );

    for (run_idx, run_values) in data.all_runs_values_ms.clone().into_iter().enumerate() {
        let series_name = format!("Run {}", run_idx + 1);
        chart = chart.series(
            Line::new()
                .name(series_name)
                .data(run_values)
                .show_symbol(false),
        );
    }

    Ok((chart, format!("{}_{}", data.save_name, metric)))
}

fn draw_min_chart(data: &PreppedVerboseData, metric: &String) -> Result<(Chart, String)> {
    let title = format!("Min {} per Tick for {}", metric, data.save_name);
    let y_axis_name = format!("Min {metric} Time (ms)");
    let tick_labels = data.ticks.iter().map(|t| t.to_string()).collect();

    Ok((
        Chart::new()
            .title(Title::new().text(title).left("center"))
            .x_axis(
                Axis::new()
                    .type_(AxisType::Category)
                    .data(tick_labels)
                    .split_line(SplitLine::new().show(false)),
            )
            .y_axis(
                Axis::new()
                    .type_(AxisType::Value)
                    .name(y_axis_name)
                    .min(data.y_min)
                    .max(data.y_max)
                    .axis_label(AxisLabel::new().formatter(JsFunction::new_with_args(
                        "value",
                        "return value.toFixed(3);",
                    ))),
            )
            .series(
                Line::new()
                    .data(data.min_values_ms.clone())
                    .show_symbol(false),
            ),
        format!("{}_{}_min", data.save_name.clone(), metric),
    ))
}

/// Helper struct for neat verbose metric data
struct PreppedVerboseData {
    save_name: String,
    ticks: Vec<u64>,
    // original_num_ticks: usize,
    all_runs_values_ms: Vec<Vec<f64>>,
    min_values_ms: Vec<f64>,
    y_min: f64,
    y_max: f64,
}

/// Helper function to prepare verbose metric data
fn prepare_metric(
    save_name: &String,
    data: &Vec<VerboseData>,
    metric: &String,
    config: &AnalyzeConfig,
) -> Result<PreppedVerboseData> {
    if data.is_empty() {
        return Err(BenchmarkErrorKind::NoVerboseData.into());
    }

    let first_csv = &data[0].csv_data;
    let mut reader = csv::Reader::from_reader(first_csv.as_bytes());
    let headers: Vec<String> = reader.headers()?.iter().map(|s| s.to_string()).collect();
    let column_index =
        headers
            .iter()
            .position(|h| h == metric)
            .ok_or(BenchmarkErrorKind::InvalidMetric {
                metric: metric.to_owned(),
            })?;

    let mut all_runs_raw_ns: Vec<Vec<f64>> = Vec::new();
    let mut ticks: Vec<u64> = Vec::new();
    for run in data {
        let mut run_raw_ns: Vec<f64> = Vec::new();
        let mut run_ticks: Vec<u64> = Vec::new();

        let mut reader = csv::Reader::from_reader(run.csv_data.as_bytes());
        for record in reader.records() {
            let rec = record?;
            let tick_str = rec.get(0).ok_or(BenchmarkErrorKind::ParseError {
                reason: "Couldn't get record[0]".to_string(),
            })?;
            let tick = tick_str.parse::<u64>()?;
            let value_str = rec
                .get(column_index)
                .ok_or(BenchmarkErrorKind::ParseError {
                    reason: "Couldn't get metric column value".to_string(),
                })?;
            let value_ns = value_str.parse::<f64>()?;

            run_ticks.push(tick);
            run_raw_ns.push(value_ns);
        }

        all_runs_raw_ns.push(run_raw_ns);
        if ticks.is_empty() {
            ticks = run_ticks;
        } else if ticks.len() != run_ticks.len() {
            return Err(BenchmarkErrorKind::TickMismatch {
                ticks: ticks.len(),
                run_ticks: run_ticks.len(),
            }
            .into());
        }
    }

    let original_num_ticks = ticks.len();
    let num_ticks = ticks.len();
    let mut min_values_ns: Vec<f64> = vec![f64::MAX; num_ticks];
    for run_values in &all_runs_raw_ns {
        for (i, &val) in run_values.iter().enumerate() {
            if i < num_ticks {
                min_values_ns[i] = min_values_ns[i].min(val);
            }
        }
    }

    // Downsample raw data *before* smoothing and conversion
    let mut all_runs_raw_ns_downsampled: Vec<Vec<f64>> = Vec::new();
    for raw_ns in all_runs_raw_ns {
        let downsampled = if let Some(max_points) = config.max_points
            && raw_ns.len() > max_points
        {
            downsample(&raw_ns, max_points)
        } else {
            raw_ns
        };
        all_runs_raw_ns_downsampled.push(downsampled);
    }

    let min_values_ns_downsampled = if let Some(max_points) = config.max_points
        && min_values_ns.len() > max_points
    {
        downsample(&min_values_ns, max_points)
    } else {
        min_values_ns
    };

    let downsampled_length = all_runs_raw_ns_downsampled[0].len();
    let step = original_num_ticks as f64 / downsampled_length as f64;
    let downsampled_ticks: Vec<u64> = (0..downsampled_length)
        .map(|i| ((i as f64 + 0.5) * step) as u64)
        .collect();

    // Now smooth the downsampled data
    let mut all_runs_values_ms: Vec<Vec<f64>> = Vec::new();
    for raw_ns in all_runs_raw_ns_downsampled {
        let smoothed_ns = utils::calculate_sma(&raw_ns, config.smooth_window);
        let smoothed_ms = smoothed_ns.iter().map(|&ns| ns / 1_000_000.0).collect();
        all_runs_values_ms.push(smoothed_ms);
    }
    let smoothed_min_ns = utils::calculate_sma(&min_values_ns_downsampled, config.smooth_window);
    let min_values_ms: Vec<f64> = smoothed_min_ns.iter().map(|&ns| ns / 1_000_000.0).collect();

    let bounds =
        utils::compute_global_metric_bounds(data, &[metric.to_string()], config.smooth_window);
    let (y_min, y_max) = bounds.get(metric).cloned().unwrap_or((0.0, 0.0));

    tracing::debug!("Prepared verbose data for: {metric}");

    Ok(PreppedVerboseData {
        save_name: save_name.to_owned(),
        ticks: downsampled_ticks,
        // original_num_ticks,
        all_runs_values_ms,
        min_values_ms,
        y_min,
        y_max,
    })
}

/// Helper function to downsample data points
fn downsample(data: &[f64], target_points: usize) -> Vec<f64> {
    let num_points = data.len();

    let bin_size = ((num_points as f64) / (target_points as f64)).ceil() as usize;

    let mut downsampled = Vec::with_capacity(target_points);
    for i in 0..(num_points / bin_size) {
        let start = i * bin_size;
        let end = (start + bin_size).min(num_points);
        let slice = &data[start..end];
        if !slice.is_empty() {
            let avg = slice.iter().sum::<f64>() / slice.len() as f64;
            downsampled.push(avg);
        }
    }

    tracing::debug!("Downsampled from: {} to: {}", data.len(), downsampled.len());
    downsampled
}
