//! Tests for configuration prioritization.
//!
//! This module tests that configuration values are resolved correctly
//! according to the priority hierarchy:
//! 1. CLI arguments (highest priority)
//! 2. Environment variables (BELT_*)
//! 3. Config file
//! 4. Default values (lowest priority)
//!
//! # Note on Test Execution
//!
//! Tests that modify environment variables use `clear_belt_env_vars()` at the start
//! to ensure a clean state. However, since environment variables are process-global,
//! these tests may interfere with each other when run in parallel. If you encounter
//! test failures, run with `--test-threads=1`:
//!
//! ```bash
//! cargo test --test config_prioritization -- --test-threads=1
//! ```
//!
//! # Environment Variable Format
//!
//! Environment variables use double underscore (`__`) to separate the section from
//! the field name. For example:
//! - `BELT_BENCHMARK__TICKS` → `benchmark.ticks`
//! - `BELT_ANALYZE__SMOOTH_WINDOW` → `analyze.smooth_window`

use belt::core::RunOrder;
use belt::core::config::{
    AnalyzeConfig, BenchmarkConfig, BlueprintConfig, GlobalConfig, SanitizeConfig,
    create_figment_from_file,
};
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

// =============================================================================
// Helper Functions
// =============================================================================

/// Creates a temporary config file with the given TOML content
fn create_config_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write config");
    file.flush().expect("Failed to flush");
    file
}

/// Clears all BELT_* environment variables
fn clear_belt_env_vars() {
    let vars_to_clear: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("BELT_"))
        .map(|(k, _)| k)
        .collect();
    for var in vars_to_clear {
        unsafe {
            std::env::remove_var(&var);
        }
    }
}

// =============================================================================
// Default Value Tests (no env vars)
// =============================================================================

#[test]
fn test_benchmark_config_default_values() {
    clear_belt_env_vars();

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.ticks, 6000, "Default ticks should be 6000");
    assert_eq!(config.runs, 5, "Default runs should be 5");
    assert_eq!(
        config.run_order,
        RunOrder::Grouped,
        "Default run_order should be Grouped"
    );
    assert!(config.pattern.is_none(), "Default pattern should be None");
    assert!(config.output.is_none(), "Default output should be None");
    assert!(config.mods_dir.is_none(), "Default mods_dir should be None");
    assert!(config.headless.is_none(), "Default headless should be None");
}

#[test]
fn test_analyze_config_default_values() {
    clear_belt_env_vars();

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
    let config = AnalyzeConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.smooth_window, 0, "Default smooth_window should be 0");
    assert_eq!(config.height, 800, "Default height should be 800");
    assert_eq!(config.width, 1200, "Default width should be 1200");
    assert!(
        config.max_points.is_none(),
        "Default max_points should be None"
    );
    assert!(
        config.verbose_metrics.is_empty(),
        "Default verbose_metrics should be empty"
    );
}

#[test]
fn test_sanitize_config_default_values() {
    clear_belt_env_vars();

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
    let config = SanitizeConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.ticks, 3600, "Default ticks should be 3600");
    assert!(config.pattern.is_none(), "Default pattern should be None");
    assert!(config.mods_dir.is_none(), "Default mods_dir should be None");
    assert!(config.headless.is_none(), "Default headless should be None");
}

#[test]
fn test_blueprint_config_default_values() {
    clear_belt_env_vars();

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
    let config = BlueprintConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.count, 0, "Default count should be 0");
    assert_eq!(config.buffer_ticks, 0, "Default buffer_ticks should be 0");
    assert!(config.pattern.is_none(), "Default pattern should be None");
    assert!(config.mods_dir.is_none(), "Default mods_dir should be None");
    assert!(config.headless.is_none(), "Default headless should be None");
}

#[test]
fn test_global_config_default_values() {
    clear_belt_env_vars();

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(&config_path, "").unwrap();

    let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
    let config = GlobalConfig::from_figment(&figment).expect("Failed to load config");

    assert!(
        config.factorio_path.is_none(),
        "Default factorio_path should be None"
    );
    assert!(!config.verbose, "Default verbose should be false");
}

// =============================================================================
// Config File Tests (no env vars)
// =============================================================================

#[test]
fn test_benchmark_config_from_file() {
    clear_belt_env_vars();

    let config_content = r#"
[benchmark]
ticks = 10000
runs = 10
pattern = "*.zip"
run_order = "sequential"
headless = true
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(
        config.ticks, 10000,
        "ticks should be loaded from config file"
    );
    assert_eq!(config.runs, 10, "runs should be loaded from config file");
    assert_eq!(
        config.pattern,
        Some("*.zip".to_string()),
        "pattern should be loaded from config file"
    );
    assert_eq!(
        config.run_order,
        RunOrder::Sequential,
        "run_order should be loaded from config file"
    );
    assert_eq!(
        config.headless,
        Some(true),
        "headless should be loaded from config file"
    );
}

#[test]
fn test_analyze_config_from_file() {
    clear_belt_env_vars();

    let config_content = r#"
[analyze]
smooth_window = 20
height = 1000
width = 1400
max_points = 5000
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = AnalyzeConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(
        config.smooth_window, 20,
        "smooth_window should be loaded from config file"
    );
    assert_eq!(
        config.height, 1000,
        "height should be loaded from config file"
    );
    assert_eq!(
        config.width, 1400,
        "width should be loaded from config file"
    );
    assert_eq!(
        config.max_points,
        Some(5000),
        "max_points should be loaded from config file"
    );
}

#[test]
fn test_sanitize_config_from_file() {
    clear_belt_env_vars();

    let config_content = r#"
[sanitize]
ticks = 7200
pattern = "test_*.zip"
headless = false
items = "iron-plate,copper-plate"
fluids = "water,steam"
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = SanitizeConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(
        config.ticks, 7200,
        "ticks should be loaded from config file"
    );
    assert_eq!(
        config.pattern,
        Some("test_*.zip".to_string()),
        "pattern should be loaded from config file"
    );
    assert_eq!(
        config.headless,
        Some(false),
        "headless should be loaded from config file"
    );
    assert_eq!(
        config.items,
        Some("iron-plate,copper-plate".to_string()),
        "items should be loaded from config file"
    );
    assert_eq!(
        config.fluids,
        Some("water,steam".to_string()),
        "fluids should be loaded from config file"
    );
}

#[test]
fn test_blueprint_config_from_file() {
    clear_belt_env_vars();

    let config_content = r#"
[blueprint]
count = 25
buffer_ticks = 240
pattern = "*.blueprint"
headless = true
bot_count = 100
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BlueprintConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.count, 25, "count should be loaded from config file");
    assert_eq!(
        config.buffer_ticks, 240,
        "buffer_ticks should be loaded from config file"
    );
    assert_eq!(
        config.pattern,
        Some("*.blueprint".to_string()),
        "pattern should be loaded from config file"
    );
    assert_eq!(
        config.headless,
        Some(true),
        "headless should be loaded from config file"
    );
    assert_eq!(
        config.bot_count,
        Some(100),
        "bot_count should be loaded from config file"
    );
}

#[test]
fn test_global_config_from_file() {
    clear_belt_env_vars();

    let config_content = r#"
[global]
factorio_path = "/opt/factorio/bin/factorio"
verbose = true
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = GlobalConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(
        config.factorio_path,
        Some("/opt/factorio/bin/factorio".into()),
        "factorio_path should be loaded from config file"
    );
    assert!(config.verbose, "verbose should be loaded from config file");
}

#[test]
fn test_partial_config_file_uses_defaults_for_missing_values() {
    clear_belt_env_vars();

    let config_content = r#"
[benchmark]
ticks = 8000
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(
        config.ticks, 8000,
        "ticks should be loaded from config file"
    );
    assert_eq!(config.runs, 5, "runs should use default value");
    assert_eq!(
        config.run_order,
        RunOrder::Grouped,
        "run_order should use default value"
    );
}

// =============================================================================
// Environment Variable Tests
// =============================================================================

#[test]
fn test_environment_variables() {
    // Clear all BELT_ env vars at the start
    clear_belt_env_vars();

    // Test 1: Benchmark env vars override config file
    {
        let config_content = r#"
[benchmark]
ticks = 5000
runs = 3
"#;
        let config_file = create_config_file(config_content);

        unsafe {
            std::env::set_var("BELT_BENCHMARK__TICKS", "15000");
            std::env::set_var("BELT_BENCHMARK__RUNS", "7");
        }

        let figment = create_figment_from_file(&config_file.path().to_path_buf())
            .expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(
            config.ticks, 15000,
            "Environment variable should override config file for ticks"
        );
        assert_eq!(
            config.runs, 7,
            "Environment variable should override config file for runs"
        );

        clear_belt_env_vars();
    }

    // Test 2: Env vars with defaults when no config file
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_BENCHMARK__TICKS", "20000");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(
            config.ticks, 20000,
            "Environment variable should be used for ticks"
        );
        assert_eq!(config.runs, 5, "Default should be used for runs");
        assert_eq!(
            config.run_order,
            RunOrder::Grouped,
            "Default should be used for run_order"
        );

        clear_belt_env_vars();
    }

    // Test 3: Analyze config env vars
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_ANALYZE__SMOOTH_WINDOW", "50");
            std::env::set_var("BELT_ANALYZE__HEIGHT", "900");
            std::env::set_var("BELT_ANALYZE__WIDTH", "1600");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = AnalyzeConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(
            config.smooth_window, 50,
            "Environment variable should set smooth_window"
        );
        assert_eq!(config.height, 900, "Environment variable should set height");
        assert_eq!(config.width, 1600, "Environment variable should set width");

        clear_belt_env_vars();
    }

    // Test 4: Sanitize config env vars
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_SANITIZE__TICKS", "5000");
            std::env::set_var("BELT_SANITIZE__HEADLESS", "false");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = SanitizeConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 5000, "Environment variable should set ticks");
        assert_eq!(
            config.headless,
            Some(false),
            "Environment variable should set headless"
        );

        clear_belt_env_vars();
    }

    // Test 5: Blueprint config env vars
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_BLUEPRINT__COUNT", "50");
            std::env::set_var("BELT_BLUEPRINT__BUFFER_TICKS", "500");
            std::env::set_var("BELT_BLUEPRINT__BOT_COUNT", "200");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = BlueprintConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.count, 50, "Environment variable should set count");
        assert_eq!(
            config.buffer_ticks, 500,
            "Environment variable should set buffer_ticks"
        );
        assert_eq!(
            config.bot_count,
            Some(200),
            "Environment variable should set bot_count"
        );

        clear_belt_env_vars();
    }

    // Test 6: Global config env vars
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_GLOBAL__VERBOSE", "true");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = GlobalConfig::from_figment(&figment).expect("Failed to load config");

        assert!(
            config.verbose,
            "Environment variable should set verbose to true"
        );

        clear_belt_env_vars();
    }

    // Test 7: Run order from env var
    {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_BENCHMARK__RUN_ORDER", "random");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(
            config.run_order,
            RunOrder::Random,
            "Environment variable should set run_order"
        );

        clear_belt_env_vars();
    }

    // Final cleanup
    clear_belt_env_vars();
}

// =============================================================================
// Prioritization Tests (env vars + config file)
// =============================================================================

#[test]
fn test_prioritization_env_overrides_config() {
    clear_belt_env_vars();

    let config_content = r#"
[benchmark]
ticks = 5000
runs = 3
pattern = "*.zip"
run_order = "sequential"
"#;

    let config_file = create_config_file(config_content);

    unsafe {
        std::env::set_var("BELT_BENCHMARK__TICKS", "12000");
        // Note: runs is NOT set via env var, so should use config file value
    }

    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    // Env var overrides config
    assert_eq!(
        config.ticks, 12000,
        "Environment variable should override config file"
    );
    // Config file values are used when not overridden by env
    assert_eq!(
        config.runs, 3,
        "Config file value should be used when env var not set"
    );
    assert_eq!(
        config.pattern,
        Some("*.zip".to_string()),
        "Config file value should be used when env var not set"
    );
    assert_eq!(
        config.run_order,
        RunOrder::Sequential,
        "Config file value should be used when env var not set"
    );

    clear_belt_env_vars();
}

#[test]
fn test_mixed_sources_partial_config() {
    clear_belt_env_vars();

    let config_content = r#"
[benchmark]
ticks = 7000
"#;

    let config_file = create_config_file(config_content);

    unsafe {
        std::env::set_var("BELT_BENCHMARK__RUNS", "8");
    }

    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    // Config file provides ticks
    assert_eq!(config.ticks, 7000, "Config file should provide ticks");
    // Environment provides runs
    assert_eq!(config.runs, 8, "Environment variable should provide runs");
    // Defaults for everything else
    assert_eq!(
        config.run_order,
        RunOrder::Grouped,
        "Default should be used for run_order"
    );
    assert!(
        config.pattern.is_none(),
        "Default should be used for pattern"
    );

    clear_belt_env_vars();
}

// =============================================================================
// Complex Config File Tests
// =============================================================================

#[test]
fn test_full_config_file_all_sections() {
    clear_belt_env_vars();

    let config_content = r#"
[global]
factorio_path = "/usr/games/factorio"
verbose = true

[benchmark]
ticks = 1000
runs = 3
pattern = "bench_*.zip"
run_order = "random"
headless = true

[analyze]
smooth_window = 15
height = 600
width = 800

[sanitize]
ticks = 1800
items = "iron-plate"
headless = false

[blueprint]
count = 5
buffer_ticks = 60
headless = true
bot_count = 50
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");

    let global = GlobalConfig::from_figment(&figment).expect("Failed to load global config");
    let benchmark =
        BenchmarkConfig::from_figment(&figment).expect("Failed to load benchmark config");
    let analyze = AnalyzeConfig::from_figment(&figment).expect("Failed to load analyze config");
    let sanitize = SanitizeConfig::from_figment(&figment).expect("Failed to load sanitize config");
    let blueprint =
        BlueprintConfig::from_figment(&figment).expect("Failed to load blueprint config");

    // Global assertions
    assert_eq!(global.factorio_path, Some("/usr/games/factorio".into()));
    assert!(global.verbose);

    // Benchmark assertions
    assert_eq!(benchmark.ticks, 1000);
    assert_eq!(benchmark.runs, 3);
    assert_eq!(benchmark.pattern, Some("bench_*.zip".to_string()));
    assert_eq!(benchmark.run_order, RunOrder::Random);
    assert_eq!(benchmark.headless, Some(true));

    // Analyze assertions
    assert_eq!(analyze.smooth_window, 15);
    assert_eq!(analyze.height, 600);
    assert_eq!(analyze.width, 800);

    // Sanitize assertions
    assert_eq!(sanitize.ticks, 1800);
    assert_eq!(sanitize.items, Some("iron-plate".to_string()));
    assert_eq!(sanitize.headless, Some(false));

    // Blueprint assertions
    assert_eq!(blueprint.count, 5);
    assert_eq!(blueprint.buffer_ticks, 60);
    assert_eq!(blueprint.headless, Some(true));
    assert_eq!(blueprint.bot_count, Some(50));
}

// =============================================================================
// RunOrder Variants Tests
// =============================================================================

#[test]
fn test_run_order_variants_from_config() {
    clear_belt_env_vars();

    for (variant_name, expected_variant) in [
        ("sequential", RunOrder::Sequential),
        ("random", RunOrder::Random),
        ("grouped", RunOrder::Grouped),
    ] {
        let config_content = format!(
            r#"
[benchmark]
run_order = "{}"
"#,
            variant_name
        );

        let config_file = create_config_file(&config_content);
        let figment = create_figment_from_file(&config_file.path().to_path_buf())
            .expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(
            config.run_order, expected_variant,
            "run_order '{}' should be parsed correctly",
            variant_name
        );
    }
}

// =============================================================================
// Path and Option Tests
// =============================================================================

#[test]
fn test_path_options_from_config_file() {
    clear_belt_env_vars();

    let config_content = r#"
[benchmark]
output = "/tmp/benchmark_results"
mods_dir = "/home/user/factorio/mods"
template_path = "/home/user/templates/report.html"
"#;

    let config_file = create_config_file(config_content);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

    assert_eq!(config.output, Some("/tmp/benchmark_results".into()));
    assert_eq!(config.mods_dir, Some("/home/user/factorio/mods".into()));
    assert_eq!(
        config.template_path,
        Some("/home/user/templates/report.html".into())
    );
}

#[test]
fn test_optional_bool_presence_in_config() {
    clear_belt_env_vars();

    let config_content_true = r#"
[benchmark]
headless = true
"#;
    let config_file = create_config_file(config_content_true);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");
    assert_eq!(config.headless, Some(true));

    let config_content_false = r#"
[benchmark]
headless = false
"#;
    let config_file = create_config_file(config_content_false);
    let figment = create_figment_from_file(&config_file.path().to_path_buf())
        .expect("Failed to create figment");
    let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");
    assert_eq!(config.headless, Some(false));
}

// =============================================================================
// Error Handling Tests
// =============================================================================

#[test]
fn test_nonexistent_config_file_error() {
    clear_belt_env_vars();

    let nonexistent_path = std::path::PathBuf::from("/nonexistent/path/config.toml");

    let result = create_figment_from_file(&nonexistent_path);

    assert!(
        result.is_err(),
        "Should return error for nonexistent config file"
    );
}
