//! Behaviour that only the real game decides: the mod's stamping and sanitizing, and how belt
//! drives Factorio. Slow; run with `cargo test -- --include-ignored` (see tests/common).

mod common;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use common::{Sandbox, blueprint_file, built, encode_blueprint, fixture};
use serde_json::{Value, json};

fn blueprint_dir(sandbox: &Sandbox, files: &[(&str, String)]) -> PathBuf {
    let dir = sandbox.path("blueprints");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, string) in files {
        std::fs::write(dir.join(name), string).unwrap();
    }
    dir
}

fn small_blueprint() -> String {
    std::fs::read_to_string(blueprint_file("small-no-drills.txt")).unwrap()
}

/// Stamps the files in `dir` into `base` and returns the output directory and belt's output.
fn stamp(
    sandbox: &Sandbox,
    dir: &Path,
    base: &Path,
    extra: &[&str],
) -> (PathBuf, std::process::Output) {
    let out = sandbox.path("out");
    let mut args = vec![
        "blueprint",
        dir.to_str().unwrap(),
        base.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    let output = sandbox.belt(&args);
    assert!(
        output.status.success(),
        "belt failed:\n{}",
        common::log(&output)
    );
    (out, output)
}

fn drill_blueprint(module: &str, quality: &str) -> String {
    encode_blueprint(&json!({ "blueprint": {
        "item": "blueprint",
        "entities": [{
            "entity_number": 1,
            "name": "electric-mining-drill",
            "position": { "x": 0.5, "y": 0.5 },
            "items": [{ "id": { "name": module, "quality": quality }, "items": { "in_inventory": [{ "inventory": 2, "stack": 0 }] } }],
        }],
    }}))
}

fn blueprint_counts(name: &str, copies: u64) -> Vec<(String, u64)> {
    let decoded = belt_blueprint_json(&std::fs::read_to_string(blueprint_file(name)).unwrap());
    let mut counts = std::collections::BTreeMap::new();
    for entity in decoded["blueprint"]["entities"].as_array().unwrap() {
        *counts
            .entry(entity["name"].as_str().unwrap().to_string())
            .or_insert(0) += copies;
    }
    counts.into_iter().collect()
}

fn belt_blueprint_json(string: &str) -> Value {
    use base64::Engine;
    use std::io::Read;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&string.trim()[1..])
        .unwrap();
    let mut json = String::new();
    flate2::read::ZlibDecoder::new(bytes.as_slice())
        .read_to_string(&mut json)
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
#[ignore = "runs Factorio"]
fn stamps_count_copies_built_not_ghosts() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "4", "--buffer-ticks", "10"],
    );

    let facts = sandbox.inspect(&out.join("small.zip"), &[]);
    let mut expected = blueprint_counts("small-no-drills.txt", 4);
    expected.push(("crash-site-spaceship".into(), 1));
    expected.sort();
    assert_eq!(
        (built(&facts), facts["ghosts"].as_u64()),
        (expected, Some(0))
    );
}

#[test]
#[ignore = "runs Factorio"]
fn building_raises_the_script_build_and_revive_events() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    let mods = sandbox.path("listener-mods");
    common::copy_dir(&fixture("event-counter"), &mods.join("event-counter"));
    let base = sandbox.create_save("base", Some(&mods));

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &[
            "--count",
            "1",
            "--buffer-ticks",
            "10",
            "--mods-dir",
            mods.to_str().unwrap(),
        ],
    );

    let facts = sandbox.inspect(&out.join("small.zip"), &[fixture("event-counter")]);
    assert_eq!(facts["events"], json!({ "built": 343, "revived": 343 }));
}

#[test]
#[ignore = "runs Factorio"]
fn the_player_is_moved_clear_before_stamping() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    // The save records the fixture mod, so the user's mods directory must hold it for syncing.
    let user_mods = sandbox.path("factorio-data/mods");
    common::copy_dir(
        &fixture("belt-test-character"),
        &user_mods.join("belt-test-character"),
    );
    let base = sandbox.create_save("base", Some(&user_mods));

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "4", "--buffer-ticks", "10"],
    );

    let facts = sandbox.inspect(&out.join("small.zip"), &[]);
    let character_x = facts["characters"][0]["x"].as_f64().unwrap();
    assert!(
        character_x > facts["area"]["right"].as_f64().unwrap(),
        "{facts}"
    );
}

#[test]
#[ignore = "runs Factorio"]
fn bot_count_robots_are_added_to_each_copys_network() {
    let sandbox = Sandbox::new();
    let roboport = encode_blueprint(&json!({ "blueprint": {
        "item": "blueprint",
        "entities": [{ "entity_number": 1, "name": "roboport", "position": { "x": 0, "y": 0 } }],
    }}));
    let dir = blueprint_dir(&sandbox, &[("port.txt", roboport)]);
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "2", "--buffer-ticks", "10", "--bot-count", "7"],
    );

    let facts = sandbox.inspect(&out.join("port.zip"), &[]);
    assert_eq!(facts["robots"], json!([7, 7]));
}

#[test]
#[ignore = "runs Factorio"]
fn the_save_is_written_after_buffer_ticks() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "1", "--buffer-ticks", "300"],
    );

    let tick = sandbox.inspect(&out.join("small.zip"), &[])["tick"]
        .as_u64()
        .unwrap();
    assert!((300..310).contains(&tick), "saved at tick {tick}");
}

#[test]
#[ignore = "runs Factorio"]
fn marker_modules_are_replaced_by_the_configured_module_and_quality() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(
        &sandbox,
        &[("drill.txt", drill_blueprint("efficiency-module", "epic"))],
    );
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &[
            "--count",
            "1",
            "--buffer-ticks",
            "1",
            "--mining-module-replacement",
            "productivity-module-2",
            "--mining-module-replacement-quality",
            "rare",
        ],
    );

    let facts = sandbox.inspect(&out.join("drill.zip"), &[]);
    assert_eq!(
        facts["drills"][0]["modules"],
        json!("productivity-module-2@rare")
    );
}

#[test]
#[ignore = "runs Factorio"]
fn a_drill_without_a_marker_is_built_without_ore() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(
        &sandbox,
        &[("drill.txt", drill_blueprint("speed-module", "normal"))],
    );
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "1", "--buffer-ticks", "1"],
    );

    let facts = sandbox.inspect(&out.join("drill.zip"), &[]);
    assert_eq!(facts["drills"][0]["resource"], json!("none"));
}

#[test]
#[ignore = "runs Factorio"]
fn a_space_age_marker_without_space_age_is_a_warning_not_ore() {
    let sandbox = Sandbox::new();
    let base_only = sandbox.path("base-only-mods");
    std::fs::create_dir_all(&base_only).unwrap();
    let builtins_off = ["space-age", "quality", "elevated-rails", "recycler"]
        .map(|name| json!({ "name": name, "enabled": false }));
    let entries: Vec<Value> = [json!({ "name": "base", "enabled": true })]
        .into_iter()
        .chain(builtins_off)
        .collect();
    let list = json!({ "mods": entries });
    std::fs::write(base_only.join("mod-list.json"), list.to_string()).unwrap();
    let base = sandbox.create_save("base", Some(&base_only));
    let dir = blueprint_dir(
        &sandbox,
        &[(
            "drill.txt",
            drill_blueprint("efficiency-module-2", "normal"),
        )],
    );

    let (out, output) = stamp(
        &sandbox,
        &dir,
        &base,
        &[
            "--count",
            "1",
            "--buffer-ticks",
            "1",
            "--mining-module-replacement-quality",
            "normal",
            "--mods-dir",
            base_only.to_str().unwrap(),
        ],
    );

    let warned = common::log_entries(&common::log(&output))
        .iter()
        .any(|entry| entry.starts_with("WARN drill: resource calcite doesn't exist in this save"));
    let resource = sandbox.inspect(&out.join("drill.zip"), &[])["drills"][0]["resource"].clone();
    assert_eq!(
        (warned, resource),
        (true, json!("none")),
        "{}",
        common::log(&output)
    );
}

#[test]
#[ignore = "runs Factorio"]
fn the_base_save_is_never_modified() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    let base = sandbox.create_save("base", None);
    let before = std::fs::read(&base).unwrap();

    stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "1", "--buffer-ticks", "1"],
    );

    assert!(std::fs::read(&base).unwrap() == before);
}

#[test]
#[ignore = "runs Factorio"]
fn the_probe_surface_is_not_in_the_save() {
    let sandbox = Sandbox::new();
    let dir = blueprint_dir(&sandbox, &[("small.txt", small_blueprint())]);
    let base = sandbox.create_save("base", None);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "1", "--buffer-ticks", "1"],
    );

    assert_eq!(
        sandbox.inspect(&out.join("small.zip"), &[])["surfaces"],
        json!(["nauvis"])
    );
}

#[test]
#[ignore = "runs Factorio"]
fn natural_ore_in_a_drills_mining_area_is_replaced_by_its_marker_ore() {
    let sandbox = Sandbox::new();
    let user_mods = sandbox.path("factorio-data/mods");
    common::copy_dir(&fixture("belt-test-ore"), &user_mods.join("belt-test-ore"));
    let base = sandbox.create_save("base", Some(&user_mods));
    let big_drill = encode_blueprint(&json!({ "blueprint": {
        "item": "blueprint",
        "entities": [{
            "entity_number": 1,
            "name": "big-mining-drill",
            "position": { "x": 0.5, "y": 0.5 },
            "items": [{ "id": { "name": "efficiency-module", "quality": "normal" }, "items": { "in_inventory": [{ "inventory": 2, "stack": 0 }] } }],
        }],
    }}));
    let dir = blueprint_dir(&sandbox, &[("drill.txt", big_drill)]);

    let (out, _) = stamp(
        &sandbox,
        &dir,
        &base,
        &["--count", "1", "--buffer-ticks", "1"],
    );

    let facts = sandbox.inspect(&out.join("drill.zip"), &[]);
    assert_eq!(facts["drills"][0]["area_resources"], json!("stone"));
}

#[test]
#[ignore = "runs Factorio"]
fn a_recorded_mod_missing_at_its_version_everywhere_fails_naming_it() {
    let sandbox = Sandbox::new();
    let user_mods = sandbox.path("factorio-data/mods");
    common::copy_dir(&fixture("belt-test-ore"), &user_mods.join("belt-test-ore"));
    let save = sandbox.create_save("recorded", Some(&user_mods));
    let info = user_mods.join("belt-test-ore/info.json");
    let bumped = std::fs::read_to_string(&info)
        .unwrap()
        .replace("0.1.0", "0.2.0");
    std::fs::write(&info, bumped).unwrap();

    let output = sandbox.belt(&[
        "sanitize",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "10",
    ]);

    let log = common::log(&output);
    let named =
        log.contains("belt-test-ore 0.1.0 isn't installed, and the mod portal doesn't have it");
    assert!(output.status.code() == Some(1) && named, "{log}");
}

#[test]
#[ignore = "runs Factorio"]
fn the_mod_sync_runs_as_the_users_mod_portal_identity() {
    let sandbox = Sandbox::new();
    let player_data = json!({ "service-username": "belt-test", "service-token": "not-a-token" });
    std::fs::write(
        sandbox.path("factorio-data/player-data.json"),
        player_data.to_string(),
    )
    .unwrap();
    let save = sandbox.create_save("plain", None);

    let output = sandbox.belt(&[
        "--verbose",
        "sanitize",
        save.to_str().unwrap(),
        "--ticks",
        "10",
    ]);

    let log = common::log(&output);
    assert!(log.contains("Local player-data.json available"), "{log}");
}

/// One sanitize run of the world fixture, shared by the sanitize tests.
struct World {
    _sandbox: Sandbox,
    report: String,
    entries: Vec<String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let sandbox = Sandbox::new();
        let mods = sandbox.path("world-mods");
        common::copy_dir(&fixture("belt-test-world"), &mods.join("belt-test-world"));
        let save = sandbox.create_save("world", Some(&mods));
        let saves = save.parent().unwrap().to_str().unwrap().to_string();
        let output = sandbox.belt(&[
            "sanitize",
            &saves,
            "--ticks",
            "60",
            "--items",
            "iron-plate",
            "--fluids",
            "water",
            "--mods-dir",
            mods.to_str().unwrap(),
        ]);
        let report = common::log(&output);
        assert!(output.status.success(), "{report}");
        World {
            entries: common::log_entries(&report),
            report,
            _sandbox: sandbox,
        }
    })
}

fn report_has(entry: &str) -> bool {
    world().entries.iter().any(|logged| logged == entry)
}

#[test]
#[ignore = "runs Factorio"]
fn reports_pollution_enabled_or_present() {
    assert!(
        report_has("WARN pollution is enabled") && world().report.contains("pollution on the map"),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn reports_enemy_expansion_enabled() {
    assert!(
        report_has("WARN enemy expansion is enabled"),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn reports_enemies_per_surface() {
    assert!(
        world()
            .entries
            .iter()
            .any(|entry| entry.starts_with("WARN nauvis: enemies: ")),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn reports_active_silos_cars_and_tanks_per_surface_and_type() {
    let lines = [
        "WARN nauvis: 1 active car",
        "WARN nauvis: 1 active rocket-silo",
        "WARN nauvis: 1 active tank",
    ];

    assert!(
        lines.iter().all(|line| report_has(line)),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn reports_infinity_entities_per_surface_and_type() {
    let lines = [
        "WARN nauvis: 1 electric-energy-interface (infinity/test entity)",
        "WARN nauvis: 1 infinity-pipe (infinity/test entity)",
    ];

    assert!(
        lines.iter().all(|line| report_has(line)),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn counts_editor_extensions_infinity_entities_but_not_hidden_ones() {
    // infinity-chest and the shown ee-infinity-chest; the hidden ee-infinity-helper doesn't count.
    assert!(
        report_has("WARN nauvis: 2 infinity-container (infinity/test entity)"),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn reports_tracked_production_and_consumption_from_the_game() {
    let lines = [
        "INFO iron-plate (normal): produced 300, consumed 120",
        "INFO iron-plate (rare): produced 120",
        "INFO iron-plate (all qualities): produced 420, consumed 120",
        "INFO water: produced 6000",
    ];

    assert!(
        lines.iter().all(|line| report_has(line)),
        "{}",
        world().report
    );
}

#[test]
#[ignore = "runs Factorio"]
fn the_save_is_never_modified() {
    let sandbox = Sandbox::new();
    let save = sandbox.create_save("plain", None);
    let before = std::fs::read(&save).unwrap();

    sandbox.belt(&[
        "sanitize",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "60",
    ]);

    assert!(std::fs::read(&save).unwrap() == before);
}

#[test]
#[ignore = "runs Factorio"]
fn an_unknown_tracked_item_fails_the_save_with_the_mods_message() {
    let sandbox = Sandbox::new();
    let save = sandbox.create_save("plain", None);

    let output = sandbox.belt(&[
        "sanitize",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "60",
        "--items",
        "unobtainium",
    ]);

    let log = common::log(&output);
    assert!(
        output.status.code() == Some(1) && log.contains("unknown item \"unobtainium\""),
        "{log}"
    );
}

#[test]
#[ignore = "runs Factorio"]
fn failed_saves_are_listed_with_their_log_tail_and_the_exit_code_is_non_zero() {
    let sandbox = Sandbox::new();
    let good = sandbox.create_save("good", None);
    std::fs::write(good.parent().unwrap().join("broken.zip"), "not a save").unwrap();

    let output = sandbox.belt(&[
        "benchmark",
        good.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "10",
        "--runs",
        "1",
        "--output",
        sandbox.path("bench").to_str().unwrap(),
    ]);

    let log = common::log(&output);
    let listed = log.contains("\nbroken: ") && log.contains("  | ");
    assert!(output.status.code() == Some(1) && listed, "{log}");
}

#[test]
#[ignore = "runs Factorio"]
fn a_failed_sync_without_a_portal_token_says_so_with_its_exit_code() {
    let sandbox = Sandbox::new(); // its player-data.json has no token
    let broken = sandbox.path("saves/broken.zip");
    std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
    std::fs::write(&broken, "not a save").unwrap();

    let output = sandbox.belt(&["sanitize", broken.to_str().unwrap(), "--ticks", "10"]);

    let log = common::log(&output);
    assert!(
        log.contains(
            "(exit code 1); it ran without a mod portal identity, as there is no mod portal token"
        ),
        "{log}"
    );
}

#[test]
#[ignore = "runs Factorio"]
fn an_interrupt_stops_factorio_and_keeps_completed_runs() {
    use std::io::BufRead;
    let sandbox = Sandbox::new();
    let save = sandbox.create_save("plain", None);
    let out = sandbox.path("bench");
    let mut child = sandbox
        .belt_command(&[
            "--verbose",
            "benchmark",
            save.parent().unwrap().to_str().unwrap(),
            "--ticks",
            "50000",
            "--runs",
            "3",
            "--output",
            out.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut lines = stdout.lines();
    // The first run completes; interrupt once the second has started.
    let mut seen = Vec::new();
    for line in lines.by_ref() {
        let line = line.unwrap();
        let started_second = line.contains("[2/3]");
        seen.push(line);
        if started_second {
            break;
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    let rest: Vec<String> = lines.map(Result::unwrap).collect();
    let status = child.wait().unwrap();

    let rows = std::fs::read_to_string(out.join("results.csv"))
        .map(|text| text.lines().count())
        .unwrap_or(0);
    assert_eq!((status.code(), rows), (Some(1), 2), "{seen:?} {rest:?}");
}

#[test]
#[ignore = "runs Factorio"]
fn the_users_factorio_is_left_as_it_was_after_a_run() {
    let sandbox = Sandbox::new();
    let save = sandbox.create_save("plain", None);
    let user_data = sandbox.path("factorio-data");
    let install = sandbox
        .factorio
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let before = (snapshot(&user_data), snapshot(&install));

    sandbox.belt(&[
        "sanitize",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "10",
    ]);
    sandbox.belt(&[
        "benchmark",
        save.parent().unwrap().to_str().unwrap(),
        "--ticks",
        "10",
        "--runs",
        "1",
        "--output",
        sandbox.path("bench").to_str().unwrap(),
    ]);

    assert!((snapshot(&user_data), snapshot(&install)) == before);
}

/// Every file under `dir` with its size and modification time.
fn snapshot(dir: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).unwrap() {
            let path = entry.unwrap().path();
            let metadata = std::fs::metadata(&path).unwrap();
            match metadata.is_dir() {
                true => pending.push(path),
                false => files.push((path, metadata.len(), metadata.modified().unwrap())),
            }
        }
    }
    files.sort();
    files
}
