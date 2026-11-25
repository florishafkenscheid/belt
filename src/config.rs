//! Settings, layered: CLI flag > `BELT_<SECTION>__<KEY>` environment variable > TOML file > default.
//! Names, sections and layering are those of B.E.L.T. 4.x.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub global: GlobalSettings,
    pub benchmark: BenchmarkSettings,
    pub sanitize: SanitizeSettings,
    pub blueprint: BlueprintSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalSettings {
    pub factorio_path: Option<PathBuf>,
    pub verbose: bool,
    pub workspace_dir: Option<PathBuf>,
    pub sanitizer_mod: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RunOrder {
    Grouped,
    Sequential,
    Random,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BenchmarkSettings {
    pub saves_dir: Option<PathBuf>,
    pub ticks: u32,
    pub runs: u32,
    pub pattern: String,
    pub output: PathBuf,
    pub template_path: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub run_order: RunOrder,
    pub verbose_metrics: Vec<String>,
    pub strip_prefix: Option<String>,
    pub record_cpu: bool,
    pub append: bool,
    pub compress: bool,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            saves_dir: None,
            ticks: 6000,
            runs: 5,
            pattern: "*".into(),
            output: PathBuf::from("."),
            template_path: None,
            mods_dir: None,
            data_dir: None,
            run_order: RunOrder::Sequential,
            verbose_metrics: Vec::new(),
            strip_prefix: None,
            record_cpu: true,
            append: false,
            compress: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SanitizeSettings {
    pub saves_dir: Option<PathBuf>,
    pub pattern: String,
    pub ticks: u32,
    pub mods_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    /// Comma-separated, as in 4.x.
    pub items: String,
    /// Comma-separated, as in 4.x.
    pub fluids: String,
}

impl Default for SanitizeSettings {
    fn default() -> Self {
        Self {
            saves_dir: None,
            pattern: "*".into(),
            ticks: 3600,
            mods_dir: None,
            data_dir: None,
            items: String::new(),
            fluids: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlueprintSettings {
    pub blueprints_dir: Option<PathBuf>,
    pub base_save_path: Option<PathBuf>,
    pub count: Option<u32>,
    pub buffer_ticks: Option<u32>,
    pub mining_module_replacement: String,
    pub mining_module_replacement_quality: String,
    pub mods_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub pattern: String,
    pub output: PathBuf,
    pub prefix: String,
    pub bot_count: u32,
}

impl Default for BlueprintSettings {
    fn default() -> Self {
        Self {
            blueprints_dir: None,
            base_save_path: None,
            count: None,
            buffer_ticks: None,
            mining_module_replacement: "speed-module-3".into(),
            mining_module_replacement_quality: "legendary".into(),
            mods_dir: None,
            data_dir: None,
            pattern: "*".into(),
            output: PathBuf::from("."),
            prefix: String::new(),
            bot_count: 0,
        }
    }
}

/// How a value is written in an environment variable.
#[derive(Clone, Copy)]
enum Kind {
    Text,
    Number,
    Bool,
    /// Comma-separated.
    List,
}

struct Field {
    section: &'static str,
    key: &'static str,
    kind: Kind,
    doc: &'static str,
}

const fn field(section: &'static str, key: &'static str, kind: Kind, doc: &'static str) -> Field {
    Field {
        section,
        key,
        kind,
        doc,
    }
}

/// Every setting, in file order. A test keeps this in step with the structs.
const FIELDS: &[Field] = &[
    field(
        "global",
        "factorio_path",
        Kind::Text,
        "Path to the Factorio binary. Unset: look in the usual install locations.",
    ),
    field(
        "global",
        "verbose",
        Kind::Bool,
        "Debug logging, including Factorio's output and every command B.E.L.T. runs.",
    ),
    field(
        "global",
        "workspace_dir",
        Kind::Text,
        "B.E.L.T.'s own data directory (mod cache, run directories). Unset: the OS default.",
    ),
    field(
        "global",
        "sanitizer_mod",
        Kind::Text,
        "Use this belt-sanitizer mod folder instead of downloading the release from the mod portal.",
    ),
    field(
        "benchmark",
        "saves_dir",
        Kind::Text,
        "Directory with the saves to benchmark.",
    ),
    field("benchmark", "ticks", Kind::Number, "Ticks per run."),
    field("benchmark", "runs", Kind::Number, "Runs per save."),
    field(
        "benchmark",
        "pattern",
        Kind::Text,
        "Glob on save file names.",
    ),
    field("benchmark", "output", Kind::Text, "Output directory."),
    field(
        "benchmark",
        "template_path",
        Kind::Text,
        "Handlebars template for the report. Unset: the built-in results.md.",
    ),
    field(
        "benchmark",
        "mods_dir",
        Kind::Text,
        "Use the mods in this directory for every save instead of each save's recorded mods.",
    ),
    field(
        "benchmark",
        "data_dir",
        Kind::Text,
        "Factorio's user data directory (mods, player-data.json). Unset: the OS default.",
    ),
    field(
        "benchmark",
        "run_order",
        Kind::Text,
        "Run order: sequential (A,B,A,B), random, or grouped (A,A,B,B).",
    ),
    field(
        "benchmark",
        "verbose_metrics",
        Kind::List,
        "Per-tick timings to export: metric names, or \"all\". Empty: none.",
    ),
    field(
        "benchmark",
        "strip_prefix",
        Kind::Text,
        "Prefix removed from reported save names.",
    ),
    field(
        "benchmark",
        "record_cpu",
        Kind::Bool,
        "Sample CPU core frequencies into cpu_freq.csv.",
    ),
    field(
        "benchmark",
        "append",
        Kind::Bool,
        "Add to existing output instead of replacing it.",
    ),
    field(
        "benchmark",
        "compress",
        Kind::Bool,
        "Write verbose metric files zstd-compressed.",
    ),
    field(
        "sanitize",
        "saves_dir",
        Kind::Text,
        "Directory with the saves to sanitize.",
    ),
    field(
        "sanitize",
        "pattern",
        Kind::Text,
        "Glob on save file names.",
    ),
    field("sanitize", "ticks", Kind::Number, "Ticks to run each save."),
    field(
        "sanitize",
        "mods_dir",
        Kind::Text,
        "Use the mods in this directory for every save instead of each save's recorded mods.",
    ),
    field(
        "sanitize",
        "data_dir",
        Kind::Text,
        "Factorio's user data directory (mods, player-data.json). Unset: the OS default.",
    ),
    field(
        "sanitize",
        "items",
        Kind::Text,
        "Comma-separated items to report production and consumption for.",
    ),
    field(
        "sanitize",
        "fluids",
        Kind::Text,
        "Comma-separated fluids to report production and consumption for.",
    ),
    field(
        "blueprint",
        "blueprints_dir",
        Kind::Text,
        "Directory with blueprint string files (given on the command line).",
    ),
    field(
        "blueprint",
        "base_save_path",
        Kind::Text,
        "Save to build the blueprints into; never modified (given on the command line).",
    ),
    field(
        "blueprint",
        "count",
        Kind::Number,
        "Copies per blueprint. Required.",
    ),
    field(
        "blueprint",
        "buffer_ticks",
        Kind::Number,
        "Ticks simulated after building, before saving. Required.",
    ),
    field(
        "blueprint",
        "mining_module_replacement",
        Kind::Text,
        "Module that replaces marker modules in mining drills.",
    ),
    field(
        "blueprint",
        "mining_module_replacement_quality",
        Kind::Text,
        "Quality of the replacement module.",
    ),
    field(
        "blueprint",
        "mods_dir",
        Kind::Text,
        "Use the mods in this directory instead of the base save's recorded mods.",
    ),
    field(
        "blueprint",
        "data_dir",
        Kind::Text,
        "Factorio's user data directory (mods, player-data.json). Unset: the OS default.",
    ),
    field(
        "blueprint",
        "pattern",
        Kind::Text,
        "Glob on blueprint file names.",
    ),
    field(
        "blueprint",
        "output",
        Kind::Text,
        "Output directory for the new saves.",
    ),
    field(
        "blueprint",
        "prefix",
        Kind::Text,
        "Prefix for output save names.",
    ),
    field(
        "blueprint",
        "bot_count",
        Kind::Number,
        "Logistic robots added to each copy's roboport network.",
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file {0} does not exist")]
    MissingFile(PathBuf),
    #[error("cannot read config file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("environment variable {name}: {message}")]
    Env { name: String, message: String },
    #[error("invalid settings: {0}")]
    Invalid(serde_json::Error),
    #[error(
        "{section}.{key} is not set: pass {flag}, set {env}, or set {key} under [{section}] in {file}"
    )]
    Missing {
        section: &'static str,
        key: &'static str,
        flag: &'static str,
        env: String,
        file: PathBuf,
    },
}

/// `value`, or an error naming every place the setting can come from.
pub fn required<T>(
    value: Option<T>,
    section: &'static str,
    key: &'static str,
    flag: &'static str,
    file: &ConfigFile,
) -> Result<T, ConfigError> {
    value.ok_or_else(|| ConfigError::Missing {
        section,
        key,
        flag,
        env: env_name(section, key),
        file: file.path().to_path_buf(),
    })
}

/// Where the config file comes from.
pub enum ConfigFile {
    /// Given by `--config` or `BELT_CONFIG`; it must exist.
    Explicit(PathBuf),
    /// The OS default location; a missing file means no file.
    Default(PathBuf),
}

impl ConfigFile {
    pub fn path(&self) -> &Path {
        match self {
            Self::Explicit(path) | Self::Default(path) => path,
        }
    }

    fn read(&self) -> Result<Option<String>, ConfigError> {
        let path = self.path();
        match (self, std::fs::read_to_string(path)) {
            (_, Ok(text)) => Ok(Some(text)),
            (Self::Default(_), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(None)
            }
            (Self::Explicit(_), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::MissingFile(path.to_path_buf()))
            }
            (_, Err(source)) => Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

/// Resolves the settings. `flags` holds only the flags the user gave, as
/// `{section: {key: value}}`; `env` looks up an environment variable.
pub fn resolve(
    flags: Value,
    env: impl Fn(&str) -> Option<String>,
    file: &ConfigFile,
) -> Result<Settings, ConfigError> {
    let mut merged = serde_json::to_value(Settings::default()).expect("defaults serialize");
    if let Some(text) = file.read()? {
        let table: toml::Table = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: file.path().to_path_buf(),
            source,
        })?;
        overlay(
            &mut merged,
            serde_json::to_value(table).expect("TOML is JSON-compatible"),
        );
    }
    overlay(&mut merged, env_layer(env)?);
    overlay(&mut merged, flags);
    serde_json::from_value(merged).map_err(ConfigError::Invalid)
}

/// `BELT_<SECTION>__<KEY>`, as in 4.x: a double underscore separates the section from the key.
fn env_name(section: &str, key: &str) -> String {
    format!("BELT_{section}__{key}").to_uppercase()
}

fn env_layer(env: impl Fn(&str) -> Option<String>) -> Result<Value, ConfigError> {
    let mut layer = Map::new();
    for field in FIELDS {
        let name = env_name(field.section, field.key);
        let Some(raw) = env(&name) else { continue };
        let value =
            parse_env(field.kind, &raw).map_err(|message| ConfigError::Env { name, message })?;
        let section = layer
            .entry(field.section)
            .or_insert_with(|| Value::Object(Map::new()));
        section
            .as_object_mut()
            .expect("sections are objects")
            .insert(field.key.into(), value);
    }
    Ok(Value::Object(layer))
}

fn parse_env(kind: Kind, raw: &str) -> Result<Value, String> {
    match kind {
        Kind::Text => Ok(Value::String(raw.into())),
        Kind::Number => raw
            .parse::<u64>()
            .map(Value::from)
            .map_err(|error| format!("{raw:?} is not a number: {error}")),
        Kind::Bool => raw
            .parse::<bool>()
            .map(Value::Bool)
            .map_err(|_| format!("{raw:?} is not true or false")),
        Kind::List => Ok(Value::Array(
            split_list(raw).into_iter().map(Value::String).collect(),
        )),
    }
}

/// Splits a comma-separated list, trimming items and dropping empty ones.
pub fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(String::from)
        .collect()
}

fn overlay(base: &mut Value, layer: Value) {
    match (base, layer) {
        (Value::Object(base), Value::Object(layer)) => {
            for (key, value) in layer {
                match base.get_mut(&key) {
                    Some(existing) if existing.is_object() => overlay(existing, value),
                    _ => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, layer) => *base = layer,
    }
}

/// A commented config file holding the binary's defaults. Unset settings are commented out.
pub fn default_file() -> String {
    let defaults = serde_json::to_value(Settings::default()).expect("defaults serialize");
    let mut text = String::from(
        "# B.E.L.T. configuration. Every setting can also be given as a flag or as\n# BELT_<SECTION>__<KEY> (for example BELT_BENCHMARK__TICKS).\n",
    );
    let mut section = "";
    for field in FIELDS {
        if field.section != section {
            section = field.section;
            text.push_str(&format!("\n[{section}]\n"));
        }
        let value = &defaults[field.section][field.key];
        text.push_str(&format!("# {}\n", field.doc));
        match value {
            Value::Null => text.push_str(&format!("# {} =\n", field.key)),
            value => text.push_str(&format!("{} = {}\n", field.key, toml_value(value))),
        }
    }
    text
}

fn toml_value(value: &Value) -> String {
    toml::Value::try_from(value)
        .expect("defaults are TOML values")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file_with(text: &str) -> (tempfile::TempDir, ConfigFile) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).unwrap();
        (dir, ConfigFile::Explicit(path))
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn a_cli_flag_beats_an_environment_variable_beats_the_config_file_beats_the_default() {
        let (_dir, file) = file_with("[benchmark]\nticks = 100\nruns = 7\n[sanitize]\nticks = 9\n");
        let env = |name: &str| match name {
            "BELT_BENCHMARK__TICKS" => Some("200".to_string()),
            "BELT_BENCHMARK__RUNS" => Some("8".to_string()),
            _ => None,
        };

        let settings = resolve(json!({"benchmark": {"ticks": 300}}), env, &file).unwrap();

        let layered = (
            settings.benchmark.ticks,
            settings.benchmark.runs,
            settings.sanitize.ticks,
            settings.blueprint.bot_count,
        );
        assert_eq!(layered, (300, 8, 9, 0));
    }

    #[test]
    fn a_generated_config_file_holds_the_binary_defaults() {
        let (_dir, file) = file_with(&default_file());

        let settings = resolve(json!({}), no_env, &file).unwrap();

        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn the_saves_directory_may_come_from_the_config_file() {
        let (_dir, file) = file_with("[benchmark]\nsaves_dir = \"/srv/saves\"\n");

        let settings = resolve(json!({}), no_env, &file).unwrap();

        assert_eq!(
            settings.benchmark.saves_dir,
            Some(PathBuf::from("/srv/saves"))
        );
    }

    #[test]
    fn a_missing_saves_directory_is_an_error_naming_where_to_set_it() {
        let file = ConfigFile::Default(PathBuf::from("/home/me/.config/belt/config.toml"));

        let error = required(
            None::<PathBuf>,
            "benchmark",
            "saves_dir",
            "the SAVES_DIR argument",
            &file,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "benchmark.saves_dir is not set: pass the SAVES_DIR argument, set BELT_BENCHMARK__SAVES_DIR, or set saves_dir under [benchmark] in /home/me/.config/belt/config.toml"
        );
    }

    #[test]
    fn an_explicit_config_path_that_does_not_exist_is_an_error() {
        let file = ConfigFile::Explicit(PathBuf::from("/nonexistent/belt.toml"));

        let result = resolve(json!({}), no_env, &file);

        assert!(
            matches!(result, Err(ConfigError::MissingFile(path)) if path == Path::new("/nonexistent/belt.toml"))
        );
    }

    #[test]
    fn a_missing_default_config_file_means_defaults() {
        let file = ConfigFile::Default(PathBuf::from("/nonexistent/belt.toml"));

        let settings = resolve(json!({}), no_env, &file).unwrap();

        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn an_environment_list_is_comma_separated() {
        let env = |name: &str| {
            (name == "BELT_BENCHMARK__VERBOSE_METRICS")
                .then(|| "wholeUpdate, entityUpdate".to_string())
        };

        let settings = resolve(
            json!({}),
            env,
            &ConfigFile::Default(PathBuf::from("/nonexistent")),
        )
        .unwrap();

        assert_eq!(
            settings.benchmark.verbose_metrics,
            vec!["wholeUpdate", "entityUpdate"]
        );
    }

    #[test]
    fn an_unparseable_environment_value_is_an_error_naming_the_variable() {
        let env = |name: &str| (name == "BELT_BENCHMARK__RUNS").then(|| "many".to_string());

        let result = resolve(
            json!({}),
            env,
            &ConfigFile::Default(PathBuf::from("/nonexistent")),
        );

        assert!(
            matches!(result, Err(ConfigError::Env { name, .. }) if name == "BELT_BENCHMARK__RUNS")
        );
    }

    #[test]
    fn every_setting_has_a_field_entry() {
        let defaults = serde_json::to_value(Settings::default()).unwrap();
        let settings: Vec<(String, String)> = defaults
            .as_object()
            .unwrap()
            .iter()
            .flat_map(|(section, keys)| {
                keys.as_object()
                    .unwrap()
                    .keys()
                    .map(move |key| (section.clone(), key.clone()))
            })
            .collect();
        let mut fields: Vec<(String, String)> = FIELDS
            .iter()
            .map(|field| (field.section.into(), field.key.into()))
            .collect();
        let mut sorted_settings = settings;
        sorted_settings.sort();
        fields.sort();

        assert_eq!(fields, sorted_settings);
    }
}
