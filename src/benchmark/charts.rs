//! Chart generation for benchmark results.
//!
//! Uses the `charming` crate to render SVG charts for UPS and improvement metrics.

use crate::{
    benchmark::parser::BenchmarkResult,
    core::{BenchmarkError, Result},
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
        return Err(BenchmarkError::NoBenchmarkResults);
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

pub fn create_verbose_charts_for_metrics(
    verbose_csv_data: &str,
    save_name: &str,
    run_index: usize,
    metrics_to_chart: &[String],
) -> Result<Vec<(Chart, String)>> {
    let mut reader = csv::Reader::from_reader(verbose_csv_data.as_bytes());

    let headers: Vec<String> = reader.headers()?.iter().map(|s| s.to_string()).collect();
    let header_map: HashMap<String, usize> = headers
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();

    let mut all_charts: Vec<(Chart, String)> = Vec::new();
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
            let mut inner_reader = csv::Reader::from_reader(verbose_csv_data.as_bytes());

            let mut ticks: Vec<u64> = Vec::new();
            let mut metric_values_ns: Vec<f64> = Vec::new();

            for record_result in inner_reader.records() {
                let record = record_result?;

                if let (Some(tick_str), Some(value_ns_str)) =
                    (record.get(0), record.get(column_index))
                {
                    if let Ok(tick) = tick_str.trim_start_matches('t').parse::<u64>() {
                        if let Ok(value_ns) = value_ns_str.parse::<f64>() {
                            ticks.push(tick);
                            metric_values_ns.push(value_ns);
                        }
                    }
                }
            }

            if ticks.is_empty() {
                tracing::warn!(
                    "No data found for metric '{}' in save {} run {}",
                    metric_name,
                    save_name,
                    run_index + 1
                );
                continue;
            }

            let metric_values_ms: Vec<f64> = metric_values_ns
                .into_iter()
                .map(|ns| ns / 1_000_000.0)
                .collect();

            let chart_title = format!(
                "{} per Tick for {} (Run {})",
                metric_name,
                save_name,
                run_index + 1
            );
            let y_axis_name = format!("{metric_name} Time (ms)");

            let chart =
                generate_single_metric_chart(ticks, metric_values_ms, &chart_title, &y_axis_name)?;
            all_charts.push((chart, metric_name));
        } else {
            tracing::warn!(
                "Request metric '{}' not found in Factorio verbose output for save {} run {}",
                metric_name,
                save_name,
                run_index + 1
            );
        }
    }

    Ok(all_charts)
}

/// Generate a line chart from verbose per-tick benchmark data
fn generate_single_metric_chart(
    ticks: Vec<u64>,
    metric_values_ms: Vec<f64>,
    chart_title: &str,
    y_axis_name: &str,
) -> Result<Chart> {
    let tick_labels: Vec<String> = ticks.iter().map(|t| t.to_string()).collect();

    let min_val = metric_values_ms
        .iter()
        .cloned()
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);
    let max_val = metric_values_ms
        .iter()
        .cloned()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or(0.0);

    let buffer = (max_val - min_val) * 0.1;

    let chart = Chart::new()
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
                .min((min_val - buffer).max(0.0)) // To ensure it doesn't go below 0
                .max(max_val + buffer)
                .axis_label(AxisLabel::new().formatter(JsFunction::new_with_args(
                    "value",
                    "return value.toFixed(3);",
                ))),
        )
        .series(Line::new().data(metric_values_ms).show_symbol(false));

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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    #[test]
    fn test_generate_verbose_chart() {
        const VERBOSE_DATA: &str = r#"tick,timestamp,wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,lightningUpdate,tileHeatingUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate,
t0,140,11080261,0,7623950,7070,522710,276560,140340,125110,0,130850,6408320,0,0,1990,1540,0,0,0,0,0,0,0,0,0,86650,890,0,0,0,1370,1570,9750,0,106700,0,2800,0,3173091,15050,272070,
t1,11086741,3044471,0,2682401,5060,267110,113670,84680,77910,0,39790,2041151,0,0,2030,1220,0,0,0,0,0,0,0,0,0,88040,830,0,0,0,1450,1490,6490,0,31860,0,3140,0,330670,9480,28920,
t2,14133402,2424960,0,2099110,3820,194460,90000,83820,76800,0,33390,1513910,0,0,1480,880,0,0,0,0,0,0,0,0,0,147930,780,0,0,0,1270,1250,4330,0,25400,0,2390,0,294020,9520,30040,"#;
        let chart = super::create_verbose_charts_for_metrics(
            VERBOSE_DATA,
            "Test Save",
            0,
            &["wholeUpdate".to_string()],
        )
        .unwrap();

        let chart_json: Value = serde_json::to_value(&chart).expect("Chart should be serializable");

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
