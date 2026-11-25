//! Whole commands against the real Factorio 2.1.25. Slow; run with
//! `cargo test -- --include-ignored` (see tests/common).

mod common;

use std::collections::BTreeMap;

use common::{Sandbox, blueprint_file, built};

#[test]
#[ignore = "runs Factorio"]
fn benchmark_of_a_generated_save_writes_all_contract_files() {
    let sandbox = Sandbox::new();
    let save = sandbox.create_save("generated", None);
    let out = sandbox.path("bench");

    let output = sandbox.belt(&[
        "benchmark",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "120",
        "--runs",
        "2",
        "--verbose-metrics",
        "wholeUpdate,entityUpdate",
        "--output",
        out.to_str().unwrap(),
    ]);

    let first_lines: Vec<String> = [
        "results.csv",
        "cpu_freq.csv",
        "generated_verbose_metrics.csv",
        "results.md",
    ]
    .iter()
    .map(|file| {
        std::fs::read_to_string(out.join(file))
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    })
    .collect();
    assert_eq!(
        first_lines,
        vec![
            "save_name,run_index,execution_time_ms,avg_ms,min_ms,max_ms,effective_ups,percentage_improvement,ticks,factorio_version,platform",
            "save_name,run_index,core_index,cpu_frequency,timestamp",
            "tick,run,wholeUpdate,entityUpdate",
            "# Factorio Benchmark Results",
        ],
        "{}",
        common::log(&output)
    );
}

#[test]
#[ignore = "runs Factorio"]
fn blueprint_then_benchmark_produces_a_running_design() {
    let sandbox = Sandbox::new();
    let base = sandbox.create_save("base", None);
    let blueprints = sandbox.path("blueprints");
    std::fs::create_dir_all(&blueprints).unwrap();
    std::fs::copy(
        blueprint_file("small-no-drills.txt"),
        blueprints.join("small.txt"),
    )
    .unwrap();
    let stamped = sandbox.path("stamped");
    sandbox.belt(&[
        "blueprint",
        blueprints.to_str().unwrap(),
        base.to_str().unwrap(),
        "--count",
        "1",
        "--buffer-ticks",
        "60",
        "--output",
        stamped.to_str().unwrap(),
    ]);
    let placed: u64 = built(&sandbox.inspect(&stamped.join("small.zip"), &[]))
        .into_iter()
        .filter(|(name, _)| name != "crash-site-spaceship")
        .map(|(_, count)| count)
        .sum();

    let benchmark = sandbox.belt(&[
        "benchmark",
        stamped.to_str().unwrap(),
        "--ticks",
        "60",
        "--runs",
        "1",
        "--output",
        sandbox.path("bench").to_str().unwrap(),
    ]);

    assert_eq!(
        (placed, benchmark.status.code()),
        (343, Some(0)),
        "{}",
        common::log(&benchmark)
    );
}

#[test]
#[ignore = "runs Factorio"]
fn mining_marker_blueprints_produce_drills_on_the_right_ore() {
    let sandbox = Sandbox::new();
    let base = sandbox.create_save("base", None);
    let blueprints = sandbox.path("blueprints");
    std::fs::create_dir_all(&blueprints).unwrap();
    for name in [
        "mining-tile-markers",
        "mining-module-markers",
        "mining-tile-beats-module",
    ] {
        std::fs::copy(
            blueprint_file(&format!("{name}.txt")),
            blueprints.join(format!("{name}.txt")),
        )
        .unwrap();
    }
    let out = sandbox.path("stamped");

    sandbox.belt(&[
        "blueprint",
        blueprints.to_str().unwrap(),
        base.to_str().unwrap(),
        "--count",
        "1",
        "--buffer-ticks",
        "1",
        "--output",
        out.to_str().unwrap(),
    ]);

    let ores = |save: &str| -> BTreeMap<String, u64> {
        let facts = sandbox.inspect(&out.join(format!("{save}.zip")), &[]);
        let mut ores = BTreeMap::new();
        for drill in facts["drills"].as_array().unwrap() {
            *ores
                .entry(drill["resource"].as_str().unwrap().to_string())
                .or_insert(0) += 1;
        }
        ores
    };
    let tiles: BTreeMap<String, u64> = [
        ("coal", 4),
        ("copper-ore", 4),
        ("iron-ore", 4),
        ("scrap", 4),
        ("stone", 4),
    ]
    .map(|(ore, count)| (ore.to_string(), count))
    .into();
    let modules: BTreeMap<String, u64> = [
        ("calcite", 2),
        ("coal", 3),
        ("copper-ore", 3),
        ("iron-ore", 3),
        ("scrap", 2),
        ("stone", 3),
        ("tungsten-ore", 2),
        ("uranium-ore", 2),
    ]
    .map(|(ore, count)| (ore.to_string(), count))
    .into();
    let found = [
        ores("mining-tile-markers"),
        ores("mining-module-markers"),
        ores("mining-tile-beats-module"),
    ];
    assert_eq!(found, [tiles.clone(), modules, tiles]);
}

#[test]
#[ignore = "runs Factorio"]
fn sanitize_reports_a_save_with_enemies_and_pollution() {
    let sandbox = Sandbox::new();
    // The save records the world fixture, so syncing finds it among the user's mods.
    let user_mods = sandbox.path("factorio-data/mods");
    common::copy_dir(
        &common::fixture("belt-test-world"),
        &user_mods.join("belt-test-world"),
    );
    let save = sandbox.create_save("polluted", Some(&user_mods));

    let output = sandbox.belt(&[
        "sanitize",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "60",
    ]);

    let report = common::log(&output);
    let entries = common::log_entries(&report);
    // The spawner may already have spawned a biter by the end of the run.
    let enemies = entries.iter().any(|entry| {
        entry.starts_with("WARN nauvis: enemies: ") && entry.ends_with("1 spawners, 1 worms")
    });
    let pollution = entries.iter().any(|entry| {
        entry.starts_with("WARN nauvis: ") && entry.ends_with(" pollution on the map")
    });
    assert!(enemies && pollution, "{report}");
}
