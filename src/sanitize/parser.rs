//! Parser for belt-sanitizer mod integration

use std::{fs, path::Path};

use serde_json::Value;

use crate::{
    Result,
    core::{config::SanitizeConfig, error::BenchmarkErrorKind, utils},
};

pub fn report(config: &SanitizeConfig) -> Result<()> {
    let path = config
        .data_dir
        .clone()
        .or_else(utils::check_sanitizer)
        .ok_or(BenchmarkErrorKind::SanitizerNotFound)?;

    parse_sanitizer(&path)?;

    Ok(())
}

fn parse_sanitizer(path: &Path) -> Result<()> {
    tracing::debug!("Found sanitizer at {}. Parsing...", &path.display());

    let contents = fs::read_to_string(path.join("sanitizer.json"))?;
    tracing::debug!("{contents}");
    let json: Value = serde_json::from_str(&contents)?;

    report_detection_warnings(&json)?;
    report_production_statistics(&json)?;

    fs::remove_dir_all(path)?;
    tracing::debug!("Removed: {}", path.display());
    Ok(())
}

fn report_detection_warnings(json: &Value) -> Result<()> {
    let snapshot = &json["snapshot"];
    let mut warnings = Vec::new();

    if snapshot["pollution_enabled"].as_bool().unwrap_or(false)
        || snapshot["total_pollution"].as_u64().unwrap_or(0) > 0
    {
        warnings.push("Pollution is enabled/present".to_string());
    }

    if snapshot["enemy_expansion_enabled"]
        .as_bool()
        .unwrap_or(false)
    {
        warnings.push("Enemy expansion is enabled".to_string());
    }

    if let Some(surfaces) = snapshot["surfaces"].as_array() {
        for surface in surfaces {
            let enemies = surface["enemy_units"].as_u64().unwrap_or(0)
                + surface["enemy_spawners"].as_u64().unwrap_or(0)
                + surface["enemy_worms"].as_u64().unwrap_or(0);

            if enemies > 0 {
                warnings.push(format!(
                    "Enemies found on surface '{}'",
                    surface["name"].as_str().unwrap_or("unknown")
                ));
            }

            if surface["active_cars"].as_u64().unwrap_or(0) > 0 {
                warnings.push(format!(
                    "Active cars found on surface '{}'",
                    surface["name"].as_str().unwrap_or("unknown")
                ));
            }
        }
    }

    if warnings.is_empty() {
        tracing::debug!("No benchmark-affecting issues found");
    } else {
        tracing::warn!("Benchmark-affecting issues found!");
        for warning in warnings {
            tracing::warn!("  - {warning}");
        }
    }

    Ok(())
}

fn report_production_statistics(json: &Value) -> Result<()> {
    let production_statistics = match json.get("production_stats") {
        Some(stats) => stats,
        None => return Err(BenchmarkErrorKind::NoProductionStatistics.into()),
    };

    let input = match production_statistics.get("input") {
        Some(input_obj) => input_obj,
        None => return Err(BenchmarkErrorKind::NoInputStatistics.into()),
    };
    let output = match production_statistics.get("output") {
        Some(output_obj) => output_obj,
        None => return Err(BenchmarkErrorKind::NoOutputStatistics.into()),
    };

    let mut items: Vec<ProductionStatistic> = Vec::new();
    let mut fluids: Vec<ProductionStatistic> = Vec::new();

    utils::process_items(input, "produced", &mut items);
    utils::process_items(output, "consumed", &mut items);

    utils::process_fluids(input, "produced", &mut fluids);
    utils::process_fluids(output, "consumed", &mut fluids);

    let mut messages = Vec::new();
    for item in items {
        if item.count > 0.0 {
            if let Some(quality) = item.quality {
                messages.push(format!(
                    "{}: {}-{} ({})",
                    item.statistic_type, quality, item.name, item.count
                ));
            } else {
                tracing::error!("{} does not have quality?", item.name);
            }
        }
    }

    for fluid in fluids {
        if fluid.count > 0.0 {
            messages.push(format!(
                "{}: {} ({})",
                fluid.statistic_type, fluid.name, fluid.count
            ));
        }
    }

    if messages.is_empty() {
        return Ok(());
    }

    tracing::info!("Production found:");
    for message in messages {
        tracing::info!("  - {message}");
    }

    Ok(())
}

#[derive(Debug)]
pub struct ProductionStatistic {
    pub statistic_type: String,
    pub name: String,
    pub quality: Option<String>,
    pub count: f32,
}
