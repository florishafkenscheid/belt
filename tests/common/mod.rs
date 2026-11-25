//! Shared harness for the tests that run the real Factorio.
//!
//! They need `BELT_TEST_SANDBOX` (the sandbox root, which holds `factorio-2.1.25`) and
//! `BELT_TEST_SANITIZER_MOD` (the mod repository). Every directory is passed explicitly, and
//! `HOME`/`XDG_*` point into the sandbox, so neither belt nor Factorio can reach the real
//! machine's defaults.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// Every fixture map is generated from this seed, so each test sees the same world (the default
/// map's enemies, for one, depend on it).
const MAP_SEED: &str = "1234567";

pub struct Sandbox {
    root: PathBuf,
    pub factorio: PathBuf,
    pub sanitizer: PathBuf,
    dir: tempfile::TempDir,
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn blueprint_file(name: &str) -> PathBuf {
    fixture("blueprints").join(name)
}

impl Sandbox {
    pub fn new() -> Self {
        let root = PathBuf::from(
            std::env::var("BELT_TEST_SANDBOX").expect("set BELT_TEST_SANDBOX to the sandbox root"),
        );
        let sanitizer = PathBuf::from(
            std::env::var("BELT_TEST_SANITIZER_MOD").expect("set BELT_TEST_SANITIZER_MOD"),
        );
        let factorio = root.join("factorio-2.1.25/bin/x64/factorio");
        let runs = root.join("test-runs");
        std::fs::create_dir_all(&runs).unwrap();
        let dir = tempfile::tempdir_in(runs).unwrap();
        std::fs::create_dir_all(dir.path().join("factorio-data/mods")).unwrap();
        std::fs::write(dir.path().join("factorio-data/player-data.json"), "{}").unwrap();
        std::fs::write(dir.path().join("config.toml"), "").unwrap();
        Self {
            root,
            factorio,
            sanitizer,
            dir,
        }
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn isolated(&self, command: &mut Command) {
        let home = self.root.join("home");
        command
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"));
    }

    /// Runs belt with every directory set explicitly.
    pub fn belt_command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_belt"));
        self.isolated(&mut command);
        command
            .arg("--config")
            .arg(self.path("config.toml"))
            .arg("--factorio-path")
            .arg(&self.factorio)
            .arg("--workspace-dir")
            .arg(self.path("workspace"))
            .arg("--sanitizer-mod")
            .arg(&self.sanitizer)
            .args(args)
            // A per-command option, so it follows the command's own arguments.
            .arg("--data-dir")
            .arg(self.path("factorio-data"));
        command
    }

    pub fn belt(&self, args: &[&str]) -> Output {
        self.belt_command(args).output().unwrap()
    }

    fn factorio_config(&self, name: &str) -> PathBuf {
        let write_data = self.path(&format!("factorio-{name}"));
        std::fs::create_dir_all(&write_data).unwrap();
        let config = write_data.join("config.ini");
        let ini = format!(
            "[path]\nread-data={}\nwrite-data={}\n",
            self.root.join("factorio-2.1.25/data").display(),
            write_data.display()
        );
        std::fs::write(&config, ini).unwrap();
        config
    }

    /// Creates a new map with the given mod directory (or none).
    pub fn create_save(&self, name: &str, mods: Option<&Path>) -> PathBuf {
        let save = self.path(&format!("saves/{name}.zip"));
        std::fs::create_dir_all(save.parent().unwrap()).unwrap();
        let mut command = Command::new(&self.factorio);
        self.isolated(&mut command);
        command
            .arg("-c")
            .arg(self.factorio_config(&format!("create-{name}")));
        let empty = self.path(&format!("mods-create-{name}"));
        std::fs::create_dir_all(&empty).unwrap();
        command
            .arg("--mod-directory")
            .arg(mods.unwrap_or(&empty))
            .arg("--map-gen-seed")
            .arg(MAP_SEED)
            .arg("--create")
            .arg(&save);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        save
    }

    /// Loads `save` with the inspect fixture (plus `extra` mod folders) and returns its facts.
    pub fn inspect(&self, save: &Path, extra: &[PathBuf]) -> Value {
        let mods = self.path(&format!("mods-inspect-{}", fastrand_name()));
        std::fs::create_dir_all(&mods).unwrap();
        copy_dir(&fixture("inspect-mod"), &mods.join("belt-inspect"));
        for folder in extra {
            copy_dir(folder, &mods.join(folder.file_name().unwrap()));
        }
        let mut command = Command::new(&self.factorio);
        self.isolated(&mut command);
        command
            .arg("-c")
            .arg(self.factorio_config("inspect"))
            .arg("--mod-directory")
            .arg(&mods)
            .arg("--test-save")
            .arg(save)
            .arg("--test-ticks")
            .arg("10");
        let output = command.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .find_map(|line| line.strip_prefix("BELT-INSPECT "))
            .unwrap_or_else(|| panic!("no facts:\n{stdout}"));
        serde_json::from_str(line).unwrap()
    }
}

fn fastrand_name() -> String {
    format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).unwrap();
        }
    }
}

pub fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A blueprint string from blueprint JSON.
pub fn encode_blueprint(value: &Value) -> String {
    use base64::Engine;
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(value.to_string().as_bytes()).unwrap();
    format!(
        "0{}",
        base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap())
    )
}

/// The number of player-force entities a save holds, by name, without characters.
pub fn built(facts: &Value) -> Vec<(String, u64)> {
    let mut counts: Vec<(String, u64)> = facts["entities"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name != "character")
        .map(|(name, count)| (name.clone(), count.as_u64().unwrap()))
        .collect();
    counts.sort();
    counts
}
