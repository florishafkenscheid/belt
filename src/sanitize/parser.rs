//! Parser for belt-sanitizer mod integration

use std::{fs, path::Path};

use serde_json::Value;

use crate::{
    Result,
    core::{error::BenchmarkErrorKind, utils},
};

pub fn report() -> Result<()> {
    if let Some(path) = utils::check_sanitizer() {
        parse_sanitizer(&path)?;
    } else {
        return Err(BenchmarkErrorKind::SanitizerNotFound.into());
    }

    Ok(())
}

fn parse_sanitizer(path: &Path) -> Result<()> {
    tracing::debug!("Found sanitizer at {}. Parsing...", &path.display());

    let contents = fs::read_to_string(path.join("sanitizer.json"))?;
    tracing::debug!("{contents}");
    let json: Value = serde_json::from_str(&contents)?;

    report_detection_warnings(&json);
    //report_production_statistics(&json);

    fs::remove_dir_all(path)?;
    Ok(())
}

fn report_detection_warnings(json: &Value) {
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
                break;
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
}
