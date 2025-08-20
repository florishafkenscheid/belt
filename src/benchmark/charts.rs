//! Chart generation for benchmark results.
//!
//! Uses the `charming` crate to render SVG charts for UPS and improvement metrics.

use crate::{
    benchmark::{parser::BenchmarkResult, runner::VerboseData},
    core::{Result, error::BenchmarkErrorKind},
};
use std::{collections::HashMap, path::Path};

use charming::{
    Chart, ImageRenderer,
    component::{Axis, Grid, Title},
    element::{
        AxisLabel, AxisType, ItemStyle, JsFunction, Label, LabelPosition, SplitArea, SplitLine,
    },
    series::{Bar, Boxplot, Line, Scatter},
};

/// Generates all charts for the given benchmark results.
///
/// Returns an error fi no results are provided.
pub async fn generate_charts(
    results: &[BenchmarkResult],
    output_dir: &Path,
    renderer: &mut ImageRenderer,
) -> Result<()> {
    if results.is_empty() {
        return Err(BenchmarkErrorKind::NoBenchmarkResults.into());
    }

    let ups_charts = generate_ups_charts(results)?; // Returns Vec<Chart>
    let base_chart = generate_base_chart(results)?; // Returns Chart

    let mut charts = Vec::new();
    charts.extend(ups_charts); // So, have to extend & push
    charts.push(base_chart);

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

pub fn create_all_verbose_charts_for_save(
    save_name: &String,
    save_verbose_data: &[VerboseData],
    metrics_to_chart: &[String],
    smooth_window: u32,
    global_metric_bounds: &HashMap<String, (f64, f64)>,
) -> Result<Vec<(Chart, String)>> {
    if save_verbose_data.is_empty() {
        return Ok(Vec::new());
    }

    let first_run_csv_data = &save_verbose_data[0].csv_data;
    let mut reader = csv::Reader::from_reader(first_run_csv_data.as_bytes());
    let headers: Vec<String> = reader.headers()?.iter().map(|s| s.to_string()).collect();
    let header_map: HashMap<String, usize> = headers
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    let mut charts_to_return: Vec<(Chart, String)> = Vec::new();

    let actual_metrics_to_chart: Vec<String> = if metrics_to_chart.contains(&"all".to_string()) {
        headers
            .into_iter()
            .filter(|h| h != "tick" && h != "timestamp") // All headers except tick and timestamp, as they are not information to be charted
            .collect()
    } else {
        metrics_to_chart.to_vec()
    };

    for metric_name in actual_metrics_to_chart {
        if let Some(&column_index) = header_map.get(&metric_name) {
            let mut all_runs_raw_values_ns: Vec<Vec<f64>> = Vec::new();
            let mut all_runs_smoothed_values_ms: Vec<Vec<f64>> = Vec::new();
            let mut all_smoothed_data_points_for_stats_ns: Vec<f64> = Vec::new();
            let mut x_axis_ticks_from_first_run: Vec<u64> = Vec::new();

            for run_data in save_verbose_data {
                let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
                let mut current_run_raw_values_ns: Vec<f64> = Vec::new();
                let mut current_run_ticks: Vec<u64> = Vec::new();

                for record_result in inner_reader.records() {
                    let record = record_result?;

                    if let (Some(tick_str), Some(value_ns_str)) =
                        (record.get(0), record.get(column_index))
                        && let Ok(tick) = tick_str.trim_start_matches('t').parse::<u64>()
                        && let Ok(value_ns) = value_ns_str.parse::<f64>()
                    {
                        current_run_ticks.push(tick);
                        current_run_raw_values_ns.push(value_ns);
                    }
                }

                if current_run_raw_values_ns.is_empty() {
                    tracing::warn!(
                        "No data found for metric '{}' in save {} run {}",
                        metric_name,
                        run_data.save_name,
                        run_data.run_index + 1
                    );
                    continue;
                }

                all_runs_raw_values_ns.push(current_run_raw_values_ns.clone());

                let smoothed_run_values_ns =
                    calculate_sma(&current_run_raw_values_ns, smooth_window);
                all_smoothed_data_points_for_stats_ns.extend(&smoothed_run_values_ns);

                let smoothed_values_ms_for_chart: Vec<f64> = smoothed_run_values_ns
                    .into_iter()
                    .map(|ns| ns / 1_000_000.0)
                    .collect();
                all_runs_smoothed_values_ms.push(smoothed_values_ms_for_chart);

                if x_axis_ticks_from_first_run.is_empty() {
                    x_axis_ticks_from_first_run = current_run_ticks;
                }
            }
            if all_runs_smoothed_values_ms.is_empty() {
                continue;
            }

            // Use global bounds if provided, otherwise fallback to local calculation
            let (min_buffered_ms, max_buffered_ms) = global_metric_bounds
                .get(&metric_name)
                .cloned()
                .unwrap_or((0.0, 0.0));

            let chart_title = format!("{metric_name} per Tick for {save_name}");
            let y_axis_name = format!("{metric_name} Time (ms)");

            let chart = generate_single_metric_chart(
                x_axis_ticks_from_first_run.clone(),
                all_runs_smoothed_values_ms,
                &chart_title,
                &y_axis_name,
                min_buffered_ms,
                max_buffered_ms,
            )?;
            charts_to_return.push((chart, metric_name.clone()));

            // Min tick chart
            let num_ticks = x_axis_ticks_from_first_run.len();
            let mut min_values_ns: Vec<f64> = vec![f64::MAX; num_ticks];

            for run_raw_data in &all_runs_raw_values_ns {
                for (tick_idx, &value) in run_raw_data.iter().enumerate() {
                    if tick_idx < num_ticks {
                        min_values_ns[tick_idx] = min_values_ns[tick_idx].min(value);
                    }
                }
            }

            let min_values_ms: Vec<f64> = min_values_ns
                .into_iter()
                .map(|ns| ns / 1_000_000.0)
                .collect();

            let min_chart_title = format!("Min {metric_name} per Tick for {save_name}");
            let min_y_axis_name = format!("Min {metric_name} Time (ms)");
            let smoothed_min_values_ms = calculate_sma(&min_values_ms, smooth_window);

            let min_chart = generate_single_metric_chart(
                x_axis_ticks_from_first_run,
                vec![smoothed_min_values_ms],
                &min_chart_title,
                &min_y_axis_name,
                min_buffered_ms,
                max_buffered_ms,
            )?;
            charts_to_return.push((min_chart, format!("{metric_name}_min")));
        } else {
            tracing::warn!(
                "Requested metric '{}' not found in Factorio verbose output for save {}",
                metric_name,
                save_name,
            );
        }
    }

    Ok(charts_to_return)
}

/// Generate a line chart from verbose per-tick benchmark data
fn generate_single_metric_chart(
    ticks: Vec<u64>,
    all_runs_metric_values_ms: Vec<Vec<f64>>,
    chart_title: &str,
    y_axis_name: &str,
    min_buffered_ms: f64,
    max_buffered_ms: f64,
) -> Result<Chart> {
    let tick_labels: Vec<String> = ticks.iter().map(|t| t.to_string()).collect();

    let mut chart = Chart::new()
        .title(Title::new().text(chart_title).left("center"))
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
                .min(min_buffered_ms)
                .max(max_buffered_ms)
                .axis_label(AxisLabel::new().formatter(JsFunction::new_with_args(
                    "value",
                    "return value.toFixed(3);",
                ))),
        );

    for (run_idx, run_values) in all_runs_metric_values_ms.into_iter().enumerate() {
        let series_name = format!("Run {}", run_idx + 1);
        chart = chart.series(
            Line::new()
                .name(series_name)
                .data(run_values)
                .show_symbol(false),
        );
    }

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

/// Calculate simple moving average
fn calculate_sma(data: &[f64], window_size: u32) -> Vec<f64> {
    if window_size == 0 || data.is_empty() {
        return data.to_vec(); // No smoothing or no data
    }

    let window_size = window_size as usize;
    let mut smoothed_data = Vec::with_capacity(data.len());
    let mut current_sum: f64 = 0.0;
    let mut window_count: usize = 0;

    for i in 0..data.len() {
        current_sum += data[i];
        window_count += 1;

        if i >= window_size {
            // Remove the oldest element that's falling out of the window
            current_sum -= data[i - window_size];
            window_count -= 1;
        }

        let avg = if window_count > 0 {
            current_sum / window_count as f64
        } else {
            0.0
        };
        smoothed_data.push(avg);
    }
    smoothed_data
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

/// Compute global min/max for each metric across all saves and runs
pub fn compute_global_metric_bounds(
    all_verbose_data: &[VerboseData],
    metrics_to_chart: &[String],
    smooth_window: u32,
) -> HashMap<String, (f64, f64)> {
    let mut bounds: HashMap<String, (f64, f64)> = HashMap::new();

    if all_verbose_data.is_empty() {
        return bounds;
    }

    let mut reader = csv::Reader::from_reader(all_verbose_data[0].csv_data.as_bytes());
    let headers: Vec<String> = reader
        .headers()
        .unwrap()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let header_map: HashMap<String, usize> = headers
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    for metric_name in metrics_to_chart {
        let mut all_smoothed_ns: Vec<f64> = Vec::new();

        if let Some(&column_index) = header_map.get(metric_name) {
            for run_data in all_verbose_data {
                let mut inner_reader = csv::Reader::from_reader(run_data.csv_data.as_bytes());
                let mut current_run_raw_values_ns: Vec<f64> = Vec::new();

                for record_result in inner_reader.records() {
                    let record = record_result.unwrap();
                    if let Some(value_ns_str) = record.get(column_index)
                        && let Ok(value_ns) = value_ns_str.parse::<f64>()
                    {
                        current_run_raw_values_ns.push(value_ns);
                    }
                }
                let smoothed_run_values_ns =
                    calculate_sma(&current_run_raw_values_ns, smooth_window);
                all_smoothed_ns.extend(smoothed_run_values_ns);
            }
        }

        if !all_smoothed_ns.is_empty() {
            let n = all_smoothed_ns.len() as f64;
            let mean = all_smoothed_ns.iter().sum::<f64>() / n;
            let stddev = (all_smoothed_ns
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();

            let min_ns = (mean - 2.0 * stddev).max(0.0);
            let max_ns = mean + 2.0 * stddev;

            let min_ms = min_ns / 1_000_000.0;
            let max_ms = max_ns / 1_000_000.0;

            let (min_ms, max_ms) = if min_ms == max_ms {
                let new_min = (min_ms * 0.9).max(0.0);
                let new_max = (max_ms * 1.1).max(0.1);
                (new_min, new_max)
            } else {
                (min_ms, max_ms)
            };

            bounds.insert(metric_name.clone(), (min_ms, max_ms));
        }
    }

    bounds
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::benchmark::runner::VerboseData;

    #[test]
    fn test_generate_verbose_chart() {
        let verbose_data: VerboseData = VerboseData { save_name: "Test Save".to_string(), run_index: 0, csv_data: r#"tick,timestamp,wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,lightningUpdate,tileHeatingUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate,
t0,140,11080261,0,7623950,7070,522710,276560,140340,125110,0,130850,6408320,0,0,1990,1540,0,0,0,0,0,0,0,0,0,86650,890,0,0,0,1370,1570,9750,0,106700,0,2800,0,3173091,15050,272070,
t1,11086741,3044471,0,2682401,5060,267110,113670,84680,77910,0,39790,2041151,0,0,2030,1220,0,0,0,0,0,0,0,0,0,88040,830,0,0,0,1450,1490,6490,0,31860,0,3140,0,330670,9480,28920,
t2,14133402,2424960,0,2099110,3820,194460,90000,83820,76800,0,33390,1513910,0,0,1480,880,0,0,0,0,0,0,0,0,0,147930,780,0,0,0,1270,1250,4330,0,25400,0,2390,0,294020,9520,30040,"#.to_string()
        };

        let smooth_window = 0;
        let save_name = "Test Save".to_string();
        let all_saves_verbose_data = [verbose_data];
        let metrics_to_chart = ["wholeUpdate".to_string()];

        let global_metric_bounds = super::compute_global_metric_bounds(
            &all_saves_verbose_data,
            &metrics_to_chart,
            smooth_window,
        );

        let charts_with_names = super::create_all_verbose_charts_for_save(
            &save_name,
            &all_saves_verbose_data,
            &metrics_to_chart,
            smooth_window,
            &global_metric_bounds,
        )
        .unwrap();

        assert_eq!(
            charts_with_names.len(),
            2,
            "Expected two charts to be created (avg & min)"
        );
        let (chart, _metric_name) = &charts_with_names[0];

        let chart_json: Value = serde_json::to_value(chart).expect("Chart should be serializable");

        let series_array = chart_json["series"][0]["data"]
            .as_array()
            .expect("Series data should be an array");
        assert_eq!(series_array.len(), 3, "Should have parsed 3 data points");

        let first_val_ms = series_array[0]
            .as_f64()
            .expect("Series data should be a float");
        let expected_val_ms = 11.080261;
        assert!(
            (first_val_ms - expected_val_ms).abs() < 0.0001,
            "The nanosecond to millisecond conversion for the first point is incorrect"
        );

        let x_axis_data = chart_json["xAxis"]["data"]
            .as_array()
            .expect("X-axis data should be an array");
        assert_eq!(x_axis_data.len(), 3, "Should have 3 x-axis labels");
        assert_eq!(
            x_axis_data[0].as_str().unwrap(),
            "0",
            "First x-axis label should be '0'"
        );
        assert_eq!(
            x_axis_data[2].as_str().unwrap(),
            "2",
            "Third x-axis label should be '2'"
        );
    }
}
