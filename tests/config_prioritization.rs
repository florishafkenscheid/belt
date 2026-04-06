//! Tests for configuration loading and precedence.

use belt::core::RunOrder;
use belt::core::config::{
    BenchmarkConfig, BlueprintConfig, GlobalConfig, SanitizeConfig, create_figment_from_file,
};
use std::io::Write;
use std::sync::{LazyLock, Mutex};
use tempfile::{NamedTempFile, TempDir};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn create_config_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write config");
    file.flush().expect("Failed to flush");
    file
}

fn clear_belt_env_vars() {
    let vars_to_clear: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("BELT_"))
        .map(|(k, _)| k)
        .collect();

    for var in vars_to_clear {
        unsafe {
            std::env::remove_var(var);
        }
    }
}

fn with_env_lock(test: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().expect("env lock poisoned");
    clear_belt_env_vars();
    test();
    clear_belt_env_vars();
}

#[test]
fn test_benchmark_config_default_values() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 6000);
        assert_eq!(config.runs, 5);
        assert_eq!(config.run_order, RunOrder::Grouped);
        assert!(config.pattern.is_none());
        assert!(config.output.is_none());
        assert!(config.mods_dir.is_none());
        assert!(config.headless.is_none());
        assert!(config.record_cpu);
        assert!(config.verbose_metrics.is_empty());
    });
}

#[test]
fn test_sanitize_config_default_values() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = SanitizeConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 3600);
        assert!(config.pattern.is_none());
        assert!(config.mods_dir.is_none());
        assert!(config.headless.is_none());
    });
}

#[test]
fn test_blueprint_config_default_values() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = BlueprintConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.count, 0);
        assert_eq!(config.buffer_ticks, 0);
        assert!(config.pattern.is_none());
        assert!(config.mods_dir.is_none());
        assert!(config.headless.is_none());
    });
}

#[test]
fn test_global_config_default_values() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = GlobalConfig::from_figment(&figment).expect("Failed to load config");

        assert!(config.factorio_path.is_none());
        assert!(!config.verbose);
    });
}

#[test]
fn test_benchmark_config_from_file() {
    with_env_lock(|| {
        let config_content = r#"
[benchmark]
ticks = 10000
runs = 10
pattern = "*.zip"
run_order = "sequential"
headless = true
record_cpu = false
"#;

        let config_file = create_config_file(config_content);
        let figment = create_figment_from_file(&config_file.path().to_path_buf())
            .expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 10000);
        assert_eq!(config.runs, 10);
        assert_eq!(config.pattern, Some("*.zip".to_string()));
        assert_eq!(config.run_order, RunOrder::Sequential);
        assert_eq!(config.headless, Some(true));
        assert!(!config.record_cpu);
    });
}

#[test]
fn test_environment_variables_override_config_file() {
    with_env_lock(|| {
        let config_content = r#"
[benchmark]
ticks = 5000
runs = 3
"#;
        let config_file = create_config_file(config_content);

        unsafe {
            std::env::set_var("BELT_BENCHMARK__TICKS", "15000");
            std::env::set_var("BELT_BENCHMARK__RUNS", "7");
            std::env::set_var("BELT_BENCHMARK__RECORD_CPU", "false");
        }

        let figment = create_figment_from_file(&config_file.path().to_path_buf())
            .expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 15000);
        assert_eq!(config.runs, 7);
        assert!(!config.record_cpu);
    });
}

#[test]
fn test_sanitize_environment_variables() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_SANITIZE__TICKS", "5000");
            std::env::set_var("BELT_SANITIZE__HEADLESS", "false");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = SanitizeConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.ticks, 5000);
        assert_eq!(config.headless, Some(false));
    });
}

#[test]
fn test_blueprint_environment_variables() {
    with_env_lock(|| {
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

        assert_eq!(config.count, 50);
        assert_eq!(config.buffer_ticks, 500);
        assert_eq!(config.bot_count, Some(200));
    });
}

#[test]
fn test_global_environment_variables() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(&config_path, "").unwrap();

        unsafe {
            std::env::set_var("BELT_GLOBAL__VERBOSE", "true");
        }

        let figment = create_figment_from_file(&config_path).expect("Failed to create figment");
        let config = GlobalConfig::from_figment(&figment).expect("Failed to load config");

        assert!(config.verbose);
    });
}

#[test]
fn test_partial_config_uses_env_and_defaults() {
    with_env_lock(|| {
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

        assert_eq!(config.ticks, 7000);
        assert_eq!(config.runs, 8);
        assert_eq!(config.run_order, RunOrder::Grouped);
        assert!(config.pattern.is_none());
    });
}

#[test]
fn test_full_config_file_all_sections() {
    with_env_lock(|| {
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
record_cpu = false

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
        let sanitize =
            SanitizeConfig::from_figment(&figment).expect("Failed to load sanitize config");
        let blueprint =
            BlueprintConfig::from_figment(&figment).expect("Failed to load blueprint config");

        assert_eq!(global.factorio_path, Some("/usr/games/factorio".into()));
        assert!(global.verbose);

        assert_eq!(benchmark.ticks, 1000);
        assert_eq!(benchmark.runs, 3);
        assert_eq!(benchmark.pattern, Some("bench_*.zip".to_string()));
        assert_eq!(benchmark.run_order, RunOrder::Random);
        assert_eq!(benchmark.headless, Some(true));
        assert!(!benchmark.record_cpu);

        assert_eq!(sanitize.ticks, 1800);
        assert_eq!(sanitize.items, Some("iron-plate".to_string()));
        assert_eq!(sanitize.headless, Some(false));

        assert_eq!(blueprint.count, 5);
        assert_eq!(blueprint.buffer_ticks, 60);
        assert_eq!(blueprint.headless, Some(true));
        assert_eq!(blueprint.bot_count, Some(50));
    });
}

#[test]
fn test_run_order_variants_from_config() {
    with_env_lock(|| {
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

            assert_eq!(config.run_order, expected_variant);
        }
    });
}

#[test]
fn test_path_options_from_config_file() {
    with_env_lock(|| {
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
    });
}

#[test]
fn test_optional_bool_presence_in_config() {
    with_env_lock(|| {
        let config_content = r#"
[benchmark]
headless = true
record_cpu = false
"#;
        let config_file = create_config_file(config_content);
        let figment = create_figment_from_file(&config_file.path().to_path_buf())
            .expect("Failed to create figment");
        let config = BenchmarkConfig::from_figment(&figment).expect("Failed to load config");

        assert_eq!(config.headless, Some(true));
        assert!(!config.record_cpu);
    });
}

#[test]
fn test_nonexistent_config_file_error() {
    with_env_lock(|| {
        let temp_dir = TempDir::new().unwrap();
        let missing = temp_dir.path().join("missing.toml");
        let err = create_figment_from_file(&missing).expect_err("Expected missing config error");
        assert!(err.to_string().contains("Configuration file not found"));
    });
}
