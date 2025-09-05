pub fn parse_sanitizer(result: &BenchmarkResult, path: &Path) -> Result<()> {
    tracing::debug!(
        "Found sanitizer for save: {}, at {}. Parsing...",
        &result.save_name,
        &path.display()
    );

    let contents = fs::read_to_string(path.join("sanitizer.json"))?;
    let json: Value = serde_json::from_str(&contents)?;

    let mode = json["mode"].as_str().unwrap_or("unknown");

    match mode {
        "detect" => report_detection_warnings(&json),
        "fix" => report_fixes_applied(&json),
        _ => println!("Unknown sanitizer mode: {mode}"),
    }

    Ok(())
}

fn report_detection_warnings(json: &Value) {
    let pre = &json["pre"];
    let mut warnings = Vec::new();

    if pre["pollution_enabled"].as_bool().unwrap_or(false)
        || pre["total_pollution"].as_u64().unwrap_or(0) > 0
    {
        warnings.push("Pollution is enabled/present".to_string());
    }

    if pre["enemy_expansion_enabled"].as_bool().unwrap_or(false) {
        warnings.push("Enemy expansion is enabled".to_string());
    }

    if let Some(surfaces) = pre["surfaces"].as_array() {
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

fn report_fixes_applied(json: &Value) {
    if let Some(actions) = json["applied_actions"].as_array() {
        if actions.is_empty() {
            tracing::debug!("No benchmark-affecting issues found");
        } else {
            tracing::debug!("Benchmark-affecting issues fixed!");
            for action in actions {
                if let Some(action_str) = action.as_str() {
                    let friendly_name = match action_str {
                        "pollution_disabled_and_cleared" => "Disabled pollution and cleared existing pollution",
                        "enemy_expansion_disabled_evolution_zeroed" => "Disabled enemy expansion and reset evolution",
                        "biters_units_killed_spawners_worms_destroyed" => "Removed all enemy units, spawners, and worms",
                        _ => action_str
                    };
                    tracing::debug!("  - {friendly_name}");
                }
            }
        }
    }
}