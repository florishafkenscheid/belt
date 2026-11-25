//! The B.E.L.T. mod cache: the `--mod-directory` of every run.
//!
//! Factorio rewrites the mod directory's `mod-list.json` on every launch and `--sync-mods`
//! writes `mod-settings.dat`, so runs never use the user's own mods directory. Mods a save needs
//! are copied into the cache once (or downloaded into it by Factorio), and before each run the
//! cache's `mod-list.json` and `mod-settings.dat` are written for exactly that run.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use super::portal::Release;
use crate::protocol::{self, Job};
use crate::version::Version;

const MOD_LIST: &str = "mod-list.json";
const MOD_SETTINGS: &str = "mod-settings.dat";

/// A mod at one version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModRef {
    pub name: String,
    pub version: Version,
}

impl std::fmt::Display for ModRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

/// A mod found on disk: a zip or a folder with an `info.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledMod {
    pub mod_ref: ModRef,
    /// The Factorio `major.minor` it is made for (`info.json`'s `factorio_version`).
    pub factorio_version: String,
    pub path: PathBuf,
}

impl InstalledMod {
    /// Whether it can load on `factorio`: Factorio only loads mods made for its own `major.minor`.
    fn loads_on(&self, factorio: Version) -> bool {
        self.factorio_version == factorio.family()
    }
}

/// The mods one save runs with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModSet {
    /// Built-in mods (from the install's data directory) to enable.
    pub builtins: BTreeSet<String>,
    /// Other mods, each at the version to load.
    pub mods: BTreeSet<ModRef>,
    /// The `mod-settings.dat` to run with; `None` runs with defaults.
    pub settings: Option<Vec<u8>>,
}

/// Whether a run loads the belt-sanitizer mod.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sanitizer {
    Enabled,
    Disabled,
}

#[derive(Debug, thiserror::Error)]
pub enum ModsError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    Malformed { path: PathBuf, message: String },
    #[error(
        "{path} speaks interface {found}, this belt needs {expected}; install a matching belt-sanitizer"
    )]
    InterfaceMismatch {
        path: PathBuf,
        found: String,
        expected: u32,
    },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ModsError + '_ {
    move |source| ModsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(Deserialize)]
struct InfoJson {
    name: String,
    version: String,
    /// Factorio's documented default for a mod that doesn't say.
    #[serde(default = "default_factorio_version")]
    factorio_version: String,
}

fn default_factorio_version() -> String {
    "0.12".into()
}

fn parse_info(text: &str, path: &Path) -> Result<InfoJson, ModsError> {
    serde_json::from_str(text).map_err(|error| ModsError::Malformed {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn read_info(dir: &Path) -> Result<InfoJson, ModsError> {
    let path = dir.join("info.json");
    let text = std::fs::read_to_string(&path).map_err(io(&path))?;
    parse_info(&text, &path)
}

/// The `info.json` of a zipped mod: the one in the zip's top folder.
fn read_zipped_info(zip: &Path) -> Result<InfoJson, ModsError> {
    let malformed = |message: String| ModsError::Malformed {
        path: zip.to_path_buf(),
        message,
    };
    let file = std::fs::File::open(zip).map_err(io(zip))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| malformed(error.to_string()))?;
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.map(|name| name.into_owned()))
        .collect::<Result<_, _>>()
        .map_err(|error| malformed(error.to_string()))?;
    let Some(name) = names
        .into_iter()
        .find(|name| name.split('/').count() == 2 && name.ends_with("/info.json"))
    else {
        return Err(malformed("no info.json in the zip's top folder".into()));
    };
    let mut text = String::new();
    std::io::Read::read_to_string(
        &mut archive
            .by_name(&name)
            .map_err(|error| malformed(error.to_string()))?,
        &mut text,
    )
    .map_err(io(zip))?;
    parse_info(&text, zip)
}

/// Mods directly in `dir`: zips and folders with an `info.json`, named and versioned by it.
pub fn installed(dir: &Path) -> Result<Vec<InstalledMod>, ModsError> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(io(dir))? {
        let path = entry.map_err(io(dir))?.path();
        let info = if path.is_dir() && path.join("info.json").is_file() {
            read_info(&path)?
        } else if path.extension().is_some_and(|extension| extension == "zip") {
            read_zipped_info(&path)?
        } else {
            continue;
        };
        let Some(version) = Version::parse(&info.version) else {
            return Err(ModsError::Malformed {
                path,
                message: format!("version {:?} is not major.minor.patch", info.version),
            });
        };
        found.push(InstalledMod {
            mod_ref: ModRef {
                name: info.name,
                version,
            },
            factorio_version: info.factorio_version,
            path,
        });
    }
    found.sort_by(|left, right| left.mod_ref.cmp(&right.mod_ref));
    Ok(found)
}

/// The built-in mods of an install (`base`, `quality`, `space-age`, …): every folder of its
/// data directory with an `info.json`, except `core`.
pub fn builtins(install_data: &Path) -> Result<BTreeSet<String>, ModsError> {
    let mut names = BTreeSet::new();
    for entry in std::fs::read_dir(install_data).map_err(io(install_data))? {
        let path = entry.map_err(io(install_data))?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name != "core" && path.join("info.json").is_file() {
            names.insert(name);
        }
    }
    Ok(names)
}

/// One entry of a `mod-list.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ListEntry {
    pub name: String,
    pub enabled: bool,
    pub version: Option<String>,
}

pub fn read_mod_list(dir: &Path) -> Result<Vec<ListEntry>, ModsError> {
    #[derive(Deserialize)]
    struct ModList {
        mods: Vec<ListEntry>,
    }
    let path = dir.join(MOD_LIST);
    let text = std::fs::read_to_string(&path).map_err(io(&path))?;
    serde_json::from_str::<ModList>(&text)
        .map(|list| list.mods)
        .map_err(|error| ModsError::Malformed {
            path,
            message: error.to_string(),
        })
}

/// What a save needs, read from the `mod-list.json` that `--sync-mods` wrote.
///
/// `--sync-mods` lists a recorded mod that isn't on disk as disabled, and a present mod the save
/// doesn't use as disabled too, so a disabled entry is needed only when it's absent from disk.
/// Built-ins resolve by name (their recorded version is the Factorio that wrote the save), and
/// belt-sanitizer is never needed: every stamped save records it, and it must not run in a
/// benchmark.
pub fn needed(
    synced: &[ListEntry],
    on_disk: &[InstalledMod],
    builtins: &BTreeSet<String>,
) -> Result<ModSet, String> {
    let present: BTreeSet<&str> = on_disk
        .iter()
        .map(|installed| installed.mod_ref.name.as_str())
        .collect();
    let mut set = ModSet::default();
    for entry in synced {
        if builtins.contains(&entry.name) {
            if entry.enabled {
                set.builtins.insert(entry.name.clone());
            }
            continue;
        }
        let wanted = entry.enabled || !present.contains(entry.name.as_str());
        if entry.name == protocol::MOD_NAME || !wanted {
            continue;
        }
        let Some(version) = entry.version.as_deref().and_then(Version::parse) else {
            return Err(format!(
                "--sync-mods listed {} without a version",
                entry.name
            ));
        };
        set.mods.insert(ModRef {
            name: entry.name.clone(),
            version,
        });
    }
    Ok(set)
}

/// A recorded mod version that can't load on the running Factorio, and the version used instead.
#[derive(Debug, Clone, PartialEq)]
pub struct Swap {
    pub recorded: ModRef,
    /// The Factorio `major.minor` the recorded version is made for.
    pub recorded_for: String,
    pub used: Version,
}

impl std::fmt::Display for Swap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Swap {
            recorded,
            recorded_for,
            used,
        } = self;
        write!(
            f,
            "{} {} → {used} ({} is for Factorio {recorded_for})",
            recorded.name, recorded.version, recorded.version
        )
    }
}

/// A recorded mod version that can't load on the running Factorio, with no version of that mod
/// that can, installed or on the mod portal.
#[derive(Debug, Clone, PartialEq)]
pub struct Unloadable {
    pub recorded: ModRef,
    /// The Factorio `major.minor` the recorded version is made for.
    pub made_for: String,
}

impl std::fmt::Display for Unloadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (for Factorio {})", self.recorded, self.made_for)
    }
}

/// What a save's recorded mods come to on the running Factorio.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolution {
    /// The mods to run with, each at one version (downloads included).
    pub chosen: BTreeSet<ModRef>,
    /// Every recorded version replaced by another; each is shown to the user.
    pub swaps: Vec<Swap>,
    /// Portal releases to download before the save can run.
    pub downloads: Vec<Release>,
    /// Recorded versions that are neither installed nor among the given releases.
    pub missing: Vec<ModRef>,
    pub unloadable: Vec<Unloadable>,
}

/// Resolves recorded mods against what is installed (`available`) and what the mod portal has
/// (`releases`, possibly none). A recorded version runs when it loads on `factorio`: as
/// installed, or downloaded. One made for another Factorio is swapped for the newest version
/// that loads: installed if there is one, otherwise the newest such release, downloaded. A
/// recorded version whose Factorio is unknown (neither installed nor released) is missing.
pub fn resolve(
    recorded: &BTreeSet<ModRef>,
    available: &[InstalledMod],
    releases: &[Release],
    factorio: Version,
) -> Resolution {
    let family = factorio.family();
    let mut resolution = Resolution::default();
    for wanted in recorded {
        let installed = available.iter().find(|found| &found.mod_ref == wanted);
        let released = releases
            .iter()
            .find(|release| release.name == wanted.name && release.version == wanted.version);
        let made_for = match (installed, released) {
            (Some(found), _) => found.factorio_version.clone(),
            (None, Some(release)) => release.factorio_version.clone(),
            (None, None) => {
                resolution.missing.push(wanted.clone());
                continue;
            }
        };
        if made_for == family {
            resolution.chosen.insert(wanted.clone());
            if installed.is_none() {
                resolution.downloads.extend(released.cloned());
            }
            continue;
        }
        let newest_installed = available
            .iter()
            .filter(|found| found.mod_ref.name == wanted.name && found.loads_on(factorio))
            .map(|found| found.mod_ref.version)
            .max();
        let newest_released = releases
            .iter()
            .filter(|release| release.name == wanted.name && release.factorio_version == family)
            .max_by_key(|release| release.version);
        let used = match (newest_installed, newest_released) {
            (Some(version), _) => version,
            (None, Some(release)) => {
                resolution.downloads.push(release.clone());
                release.version
            }
            (None, None) => {
                resolution.unloadable.push(Unloadable {
                    recorded: wanted.clone(),
                    made_for,
                });
                continue;
            }
        };
        resolution.chosen.insert(ModRef {
            name: wanted.name.clone(),
            version: used,
        });
        resolution.swaps.push(Swap {
            recorded: wanted.clone(),
            recorded_for: made_for,
            used,
        });
    }
    resolution
}

/// Copies `wanted` from `source` (the user's mods) into `cache`. Returns whether it was there.
/// A zip already in the cache is kept: a `name_version.zip` doesn't change. Folder mods are
/// copied every time, because their content can change without a version bump.
pub fn copy_into_cache(wanted: &ModRef, source: &Path, cache: &Path) -> Result<bool, ModsError> {
    if !source.is_dir() {
        return Ok(false);
    }
    let Some(found) = installed(source)?
        .into_iter()
        .find(|installed| &installed.mod_ref == wanted)
    else {
        return Ok(false);
    };
    copy_mod(&found, cache)?;
    Ok(true)
}

fn copy_mod(found: &InstalledMod, cache: &Path) -> Result<(), ModsError> {
    let name = format!("{}_{}", found.mod_ref.name, found.mod_ref.version);
    if found.path.is_dir() {
        let target = cache.join(name);
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(io(&target))?;
        }
        return copy_dir(&found.path, &target, Entries::All);
    }
    let target = cache.join(format!("{name}.zip"));
    if !target.exists() {
        std::fs::copy(&found.path, &target).map_err(io(&target))?;
    }
    Ok(())
}

/// Which entries of a folder to copy.
#[derive(Clone, Copy, PartialEq)]
enum Entries {
    All,
    /// Leaves out hidden entries (`.git` and the like), as a checkout of the mod has them.
    Visible,
}

fn copy_dir(from: &Path, to: &Path, entries: Entries) -> Result<(), ModsError> {
    std::fs::create_dir_all(to).map_err(io(to))?;
    for entry in std::fs::read_dir(from).map_err(io(from))? {
        let path = entry.map_err(io(from))?.path();
        let name = path.file_name().expect("entries have names");
        if entries == Entries::Visible && name.to_string_lossy().starts_with('.') {
            continue;
        }
        let target = to.join(name);
        if path.is_dir() {
            copy_dir(&path, &target, entries)?;
        } else {
            std::fs::copy(&path, &target).map_err(io(&target))?;
        }
    }
    Ok(())
}

/// The set for `--mods <dir>`, following Factorio's own rule for that directory: a mod is
/// enabled as its `mod-list.json` says (all of them when it has none), at the version that list
/// pins, or else the newest installed. Built-ins are enabled the same way. The chosen mods are
/// copied into the cache, and the directory's `mod-settings.dat` is used.
pub fn from_directory(
    dir: &Path,
    cache: &Path,
    builtins: &BTreeSet<String>,
) -> Result<ModSet, ModsError> {
    let list = if dir.join(MOD_LIST).is_file() {
        Some(read_mod_list(dir)?)
    } else {
        None
    };
    let entry = |name: &str| {
        list.as_ref()
            .and_then(|list| list.iter().find(|entry| entry.name == name))
    };
    let enabled = |name: &str| list.is_none() || entry(name).is_some_and(|entry| entry.enabled);
    let mut set = ModSet {
        builtins: builtins
            .iter()
            .filter(|name| enabled(name))
            .cloned()
            .collect(),
        ..Default::default()
    };
    let available = installed(dir)?;
    let names: BTreeSet<&str> = available
        .iter()
        .map(|found| found.mod_ref.name.as_str())
        .collect();
    for name in names {
        if name == protocol::MOD_NAME || !enabled(name) {
            continue;
        }
        let versions = available.iter().filter(|found| found.mod_ref.name == name);
        let chosen = match entry(name).and_then(|entry| entry.version.as_deref()) {
            Some(pinned) => {
                let Some(found) = versions
                    .clone()
                    .find(|found| found.mod_ref.version.to_string() == pinned)
                else {
                    return Err(ModsError::Malformed {
                        path: dir.join(MOD_LIST),
                        message: format!("pins {name} {pinned}, which the directory doesn't hold"),
                    });
                };
                found
            }
            None => versions
                .max_by_key(|found| found.mod_ref.version)
                .expect("every name has a version"),
        };
        copy_mod(chosen, cache)?;
        set.mods.insert(chosen.mod_ref.clone());
    }
    let settings_path = dir.join(MOD_SETTINGS);
    set.settings = if settings_path.is_file() {
        Some(std::fs::read(&settings_path).map_err(io(&settings_path))?)
    } else {
        None
    };
    Ok(set)
}

/// The cache's `mod-settings.dat`, as `--sync-mods` just wrote it.
pub fn read_settings(cache: &Path) -> Result<Option<Vec<u8>>, ModsError> {
    let path = cache.join(MOD_SETTINGS);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ModsError::Io { path, source }),
    }
}

/// Prepares the cache for one run of a save: its startup settings, and a `mod-list.json` that
/// names every built-in and every cached mod, so Factorio enables nothing on its own.
pub fn prepare_run(
    cache: &Path,
    builtins: &BTreeSet<String>,
    set: &ModSet,
    sanitizer: Sanitizer,
) -> Result<(), ModsError> {
    let settings_path = cache.join(MOD_SETTINGS);
    match &set.settings {
        Some(bytes) => std::fs::write(&settings_path, bytes).map_err(io(&settings_path))?,
        None if settings_path.exists() => {
            std::fs::remove_file(&settings_path).map_err(io(&settings_path))?
        }
        None => {}
    }
    let mut entries: Vec<Value> = builtins
        .iter()
        .map(|name| json!({ "name": name, "enabled": set.builtins.contains(name) }))
        .collect();
    let cached: BTreeSet<String> = installed(cache)?
        .into_iter()
        .map(|found| found.mod_ref.name)
        .collect();
    for name in cached {
        if name == protocol::MOD_NAME {
            continue;
        }
        match set.mods.iter().find(|wanted| wanted.name == name) {
            Some(wanted) => entries.push(
                json!({ "name": name, "enabled": true, "version": wanted.version.to_string() }),
            ),
            None => entries.push(json!({ "name": name, "enabled": false })),
        }
    }
    entries.push(json!({ "name": protocol::MOD_NAME, "enabled": sanitizer == Sanitizer::Enabled }));
    let path = cache.join(MOD_LIST);
    std::fs::write(&path, json!({ "mods": entries }).to_string()).map_err(io(&path))
}

/// The belt-sanitizer mod the CLI runs.
#[derive(Debug, Clone, PartialEq)]
pub enum SanitizerSource {
    /// A mod folder (`--sanitizer-mod`).
    Folder(PathBuf),
    /// A release zip from the mod portal.
    Zip(PathBuf),
}

/// Refuses a belt-sanitizer whose `info.json` major version isn't [`protocol::INTERFACE`].
pub fn check_sanitizer(source: &SanitizerSource) -> Result<(), ModsError> {
    let (info, path) = match source {
        SanitizerSource::Folder(folder) => (read_info(folder)?, folder.join("info.json")),
        SanitizerSource::Zip(zip) => (read_zipped_info(zip)?, zip.clone()),
    };
    let major = info.version.split('.').next().unwrap_or_default();
    let matches = info.name == protocol::MOD_NAME && major == protocol::INTERFACE.to_string();
    if !matches {
        return Err(ModsError::InterfaceMismatch {
            path,
            found: format!("{} {}", info.name, info.version),
            expected: protocol::INTERFACE,
        });
    }
    Ok(())
}

/// Puts a fresh copy of belt-sanitizer into the cache, as a folder holding `job` as its
/// `job.lua`. A folder's hidden entries are left out; a zip's top folder is extracted.
pub fn install_sanitizer(
    source: &SanitizerSource,
    cache: &Path,
    job: &Job,
) -> Result<(), ModsError> {
    let target = cache.join(protocol::MOD_NAME);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(io(&target))?;
    }
    match source {
        SanitizerSource::Folder(folder) => copy_dir(folder, &target, Entries::Visible)?,
        SanitizerSource::Zip(zip) => extract_mod(zip, &target)?,
    }
    let job_path = target.join("job.lua");
    std::fs::write(&job_path, protocol::job_lua(job)).map_err(io(&job_path))
}

/// Extracts a mod zip's top folder (`name_version/…`) into `target`.
fn extract_mod(zip: &Path, target: &Path) -> Result<(), ModsError> {
    let malformed = |message: String| ModsError::Malformed {
        path: zip.to_path_buf(),
        message,
    };
    let file = std::fs::File::open(zip).map_err(io(zip))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| malformed(error.to_string()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| malformed(error.to_string()))?;
        let Some(inside) = entry.enclosed_name() else {
            return Err(malformed(format!(
                "unsafe path {:?} in the zip",
                entry.name()
            )));
        };
        let relative: PathBuf = inside.iter().skip(1).collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let path = target.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(io(&path))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let mut output = std::fs::File::create(&path).map_err(io(&path))?;
        std::io::copy(&mut entry, &mut output).map_err(io(&path))?;
    }
    Ok(())
}

/// Mod portal credentials for `--sync-mods` downloads.
#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    pub username: String,
    pub token: String,
}

/// Reads `service-username` and `service-token` from Factorio's `player-data.json`.
pub fn read_credentials(player_data: &Path) -> Result<Credentials, String> {
    let no_token = || {
        format!(
            "no mod portal token (service-username and service-token) in {}",
            player_data.display()
        )
    };
    let text = std::fs::read_to_string(player_data).map_err(|_| no_token())?;
    let value: Value = serde_json::from_str(&text).map_err(|_| no_token())?;
    let field = |key: &str| {
        value[key]
            .as_str()
            .filter(|text| !text.is_empty())
            .map(String::from)
    };
    match (field("service-username"), field("service-token")) {
        (Some(username), Some(token)) => Ok(Credentials { username, token }),
        _ => Err(no_token()),
    }
}

impl Credentials {
    /// Writes a `player-data.json` into `write_data` holding only these two fields, so a run's
    /// `--sync-mods` downloads as the user. Readable by the owner only, on Unix.
    pub fn write_player_data(&self, write_data: &Path) -> std::io::Result<()> {
        let json = serde_json::json!({
            "service-username": self.username,
            "service-token": self.token,
        });
        std::fs::create_dir_all(write_data)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(write_data.join("player-data.json"))?;
        std::io::Write::write_all(&mut file, json.to_string().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACTORIO: Version = Version::new(2, 1, 19);

    fn mod_ref(name: &str, version: &str) -> ModRef {
        ModRef {
            name: name.into(),
            version: Version::parse(version).unwrap(),
        }
    }

    /// An installed mod made for Factorio `factorio` (`major.minor`).
    fn made_for(name: &str, version: &str, factorio: &str) -> InstalledMod {
        InstalledMod {
            mod_ref: mod_ref(name, version),
            factorio_version: factorio.into(),
            path: PathBuf::from(format!("/mods/{name}_{version}.zip")),
        }
    }

    fn on_disk(name: &str, version: &str) -> InstalledMod {
        made_for(name, version, "2.1")
    }

    /// Writes `dir/name_version.zip` holding `name_version/info.json`, as the mod portal ships it.
    fn zipped_mod(dir: &Path, name: &str, version: &str) {
        let file = std::fs::File::create(dir.join(format!("{name}_{version}.zip"))).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(format!("{name}_{version}/info.json"), options)
            .unwrap();
        let info = json!({ "name": name, "version": version, "title": name, "author": "test", "factorio_version": "2.1" });
        std::io::Write::write_all(&mut zip, info.to_string().as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn entry(name: &str, enabled: bool, version: &str) -> ListEntry {
        ListEntry {
            name: name.into(),
            enabled,
            version: Some(version.into()),
        }
    }

    fn builtins() -> BTreeSet<String> {
        ["base", "space-age"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_save_needs_its_enabled_mods_and_its_recorded_mods_missing_from_disk() {
        let synced = [
            entry("base", true, "2.1.20"),
            entry("enabled", true, "1.0.0"),
            entry("absent", false, "2.0.0"),
            entry("unused", false, "3.0.0"),
        ];

        let set = needed(&synced, &[on_disk("unused", "3.0.0")], &builtins()).unwrap();

        let expected: BTreeSet<ModRef> =
            [mod_ref("absent", "2.0.0"), mod_ref("enabled", "1.0.0")].into();
        assert_eq!(
            (set.builtins, set.mods),
            (["base".to_string()].into(), expected)
        );
    }

    #[test]
    fn the_sanitizer_recorded_by_a_stamped_save_is_never_needed() {
        let synced = [entry(protocol::MOD_NAME, false, "1.0.0")];

        assert!(needed(&synced, &[], &builtins()).unwrap().mods.is_empty());
    }

    #[test]
    fn a_recorded_mod_at_another_version_is_not_run_silently() {
        let recorded = [mod_ref("rails", "0.6.0")].into();

        let resolution = resolve(&recorded, &[on_disk("rails", "0.6.1")], &[], FACTORIO);

        assert_eq!(
            (resolution.missing, resolution.chosen),
            (vec![mod_ref("rails", "0.6.0")], BTreeSet::new())
        );
    }

    #[test]
    fn a_recorded_version_that_loads_is_kept() {
        let recorded = [mod_ref("flib", "0.17.2")].into();

        let resolution = resolve(
            &recorded,
            &[on_disk("flib", "0.17.2"), on_disk("flib", "0.18.0")],
            &[],
            FACTORIO,
        );

        assert_eq!(resolution.chosen, [mod_ref("flib", "0.17.2")].into());
    }

    #[test]
    fn a_recorded_version_for_an_older_factorio_is_swapped_for_the_newest_that_loads() {
        let recorded = [mod_ref("EditorExtensions", "2.4.2")].into();
        let available = [
            made_for("EditorExtensions", "2.4.2", "2.0"),
            made_for("EditorExtensions", "2.5.0", "2.1"),
            made_for("EditorExtensions", "2.6.1", "2.1"),
            made_for("EditorExtensions", "2.7.0", "2.2"),
        ];

        let resolution = resolve(&recorded, &available, &[], FACTORIO);

        let swaps: Vec<String> = resolution.swaps.iter().map(ToString::to_string).collect();
        assert_eq!(
            (resolution.chosen, swaps),
            (
                [mod_ref("EditorExtensions", "2.6.1")].into(),
                vec!["EditorExtensions 2.4.2 → 2.6.1 (2.4.2 is for Factorio 2.0)".to_string()]
            )
        );
    }

    #[test]
    fn a_recorded_mod_with_no_version_that_loads_is_unloadable() {
        let recorded = [mod_ref("flib", "0.16.0")].into();
        let releases = [release("flib", "0.16.0", "2.0")];

        let resolution = resolve(
            &recorded,
            &[made_for("flib", "0.16.0", "2.0")],
            &releases,
            FACTORIO,
        );

        let unloadable = Unloadable {
            recorded: mod_ref("flib", "0.16.0"),
            made_for: "2.0".into(),
        };
        assert_eq!(resolution.unloadable, vec![unloadable]);
    }

    #[test]
    fn a_zipped_mod_is_named_and_versioned_by_its_info_json() {
        let dir = tempfile::tempdir().unwrap();
        zipped_mod(dir.path(), "flib", "0.17.2");

        let found = installed(dir.path()).unwrap();

        assert_eq!(
            (found[0].mod_ref.clone(), found[0].factorio_version.as_str()),
            (mod_ref("flib", "0.17.2"), "2.1")
        );
    }

    #[test]
    fn a_mod_directory_runs_the_version_its_mod_list_pins() {
        let (dir, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        for version in ["2.4.2", "2.6.1"] {
            zipped_mod(dir.path(), "EditorExtensions", version);
        }
        let list = json!({ "mods": [{ "name": "EditorExtensions", "enabled": true, "version": "2.4.2" }] });
        std::fs::write(dir.path().join(MOD_LIST), list.to_string()).unwrap();

        let set = from_directory(dir.path(), cache.path(), &builtins()).unwrap();

        assert_eq!(set.mods, [mod_ref("EditorExtensions", "2.4.2")].into());
    }

    #[test]
    fn a_mod_directory_runs_the_newest_version_without_a_pin() {
        let (dir, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        for version in ["2.4.2", "2.10.0", "2.6.1"] {
            zipped_mod(dir.path(), "EditorExtensions", version);
        }

        let set = from_directory(dir.path(), cache.path(), &builtins()).unwrap();

        assert_eq!(set.mods, [mod_ref("EditorExtensions", "2.10.0")].into());
    }

    #[test]
    fn a_download_without_a_portal_token_fails_naming_where_it_looked() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("player-data.json");
        std::fs::write(&path, r#"{"service-username": "someone"}"#).unwrap();

        let error = read_credentials(&path).unwrap_err();

        assert!(error.contains(&path.display().to_string()), "{error}");
    }

    #[test]
    fn a_runs_player_data_holds_only_the_portal_identity() {
        let (user, run) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let path = user.path().join("player-data.json");
        let player_data = json!({
            "service-username": "someone", "service-token": "secret", "last-played-version": {},
        });
        std::fs::write(&path, player_data.to_string()).unwrap();

        read_credentials(&path)
            .unwrap()
            .write_player_data(run.path())
            .unwrap();

        let written: Value =
            serde_json::from_slice(&std::fs::read(run.path().join("player-data.json")).unwrap())
                .unwrap();
        assert_eq!(
            written,
            json!({ "service-username": "someone", "service-token": "secret" })
        );
    }

    #[test]
    fn each_save_runs_with_its_own_startup_settings() {
        let cache = tempfile::tempdir().unwrap();
        let first = ModSet {
            settings: Some(b"first".to_vec()),
            ..Default::default()
        };
        let second = ModSet {
            settings: Some(b"second".to_vec()),
            ..Default::default()
        };
        prepare_run(cache.path(), &builtins(), &first, Sanitizer::Disabled).unwrap();

        prepare_run(cache.path(), &builtins(), &second, Sanitizer::Disabled).unwrap();

        assert_eq!(
            std::fs::read(cache.path().join(MOD_SETTINGS)).unwrap(),
            b"second"
        );
    }

    #[test]
    fn the_run_mod_list_names_every_cached_mod_and_pins_the_needed_version() {
        let cache = tempfile::tempdir().unwrap();
        zipped_mod(cache.path(), "rails", "0.6.0");
        zipped_mod(cache.path(), "other", "1.0.0");
        let set = ModSet {
            builtins: ["base".to_string()].into(),
            mods: [mod_ref("rails", "0.6.0")].into(),
            settings: None,
        };

        prepare_run(cache.path(), &builtins(), &set, Sanitizer::Enabled).unwrap();

        assert_eq!(
            std::fs::read_to_string(cache.path().join(MOD_LIST)).unwrap(),
            r#"{"mods":[{"enabled":true,"name":"base"},{"enabled":false,"name":"space-age"},{"enabled":false,"name":"other"},{"enabled":true,"name":"rails","version":"0.6.0"},{"enabled":true,"name":"belt-sanitizer"}]}"#
        );
    }

    #[test]
    fn the_cli_refuses_a_mod_with_an_incompatible_interface_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("info.json"),
            r#"{"name": "belt-sanitizer", "version": "1.3.0"}"#,
        )
        .unwrap();

        assert!(matches!(
            check_sanitizer(&SanitizerSource::Folder(dir.path().to_path_buf())),
            Err(ModsError::InterfaceMismatch { .. })
        ));
    }

    #[test]
    fn a_portal_release_zip_is_installed_as_the_mod_folder_with_its_job() {
        let (downloads, cache) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        zipped_mod(downloads.path(), "belt-sanitizer", "2.0.0");
        let zip = downloads.path().join("belt-sanitizer_2.0.0.zip");
        let job = Job::Sanitize(protocol::SanitizeJob {
            ticks: 1,
            items: vec![],
            fluids: vec![],
        });

        install_sanitizer(&SanitizerSource::Zip(zip), cache.path(), &job).unwrap();

        let mut installed: Vec<String> = std::fs::read_dir(cache.path().join(protocol::MOD_NAME))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        installed.sort();
        assert_eq!(installed, vec!["info.json", "job.lua"]);
    }

    #[test]
    fn the_sanitizer_copy_leaves_out_hidden_entries() {
        let source = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join(".git")).unwrap();
        std::fs::write(source.path().join("control.lua"), "").unwrap();
        let job = Job::Sanitize(protocol::SanitizeJob {
            ticks: 1,
            items: vec![],
            fluids: vec![],
        });

        install_sanitizer(
            &SanitizerSource::Folder(source.path().to_path_buf()),
            cache.path(),
            &job,
        )
        .unwrap();

        let mut copied: Vec<String> = std::fs::read_dir(cache.path().join(protocol::MOD_NAME))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        copied.sort();
        assert_eq!(copied, vec!["control.lua", "job.lua"]);
    }

    use super::super::portal::release;

    fn downloads(resolution: &Resolution) -> Vec<String> {
        resolution
            .downloads
            .iter()
            .map(|release| format!("{} {}", release.name, release.version))
            .collect()
    }

    #[test]
    fn a_missing_recorded_version_made_for_another_factorio_is_swapped_for_an_installed_one() {
        let recorded = [mod_ref("RateCalculator", "3.3.7")].into();
        let releases = [
            release("RateCalculator", "3.3.7", "2.0"),
            release("RateCalculator", "3.4.2", "2.1"),
        ];

        let resolution = resolve(
            &recorded,
            &[on_disk("RateCalculator", "3.4.0")],
            &releases,
            FACTORIO,
        );

        let swaps: Vec<String> = resolution.swaps.iter().map(ToString::to_string).collect();
        assert_eq!(
            (swaps, downloads(&resolution)),
            (
                vec!["RateCalculator 3.3.7 → 3.4.0 (3.3.7 is for Factorio 2.0)".to_string()],
                vec![]
            )
        );
    }

    #[test]
    fn without_an_installed_replacement_the_newest_release_that_loads_is_downloaded() {
        let recorded = [mod_ref("RateCalculator", "3.3.7")].into();
        let releases = [
            release("RateCalculator", "3.3.7", "2.0"),
            release("RateCalculator", "3.4.2", "2.1"),
            release("RateCalculator", "3.4.1", "2.1"),
        ];

        let resolution = resolve(&recorded, &[], &releases, FACTORIO);

        assert_eq!(
            (resolution.chosen.clone(), downloads(&resolution)),
            (
                [mod_ref("RateCalculator", "3.4.2")].into(),
                vec!["RateCalculator 3.4.2".to_string()]
            )
        );
    }

    #[test]
    fn a_missing_recorded_version_that_loads_is_downloaded_as_recorded() {
        let recorded = [mod_ref("flib", "0.17.2")].into();
        let releases = [
            release("flib", "0.17.2", "2.1"),
            release("flib", "0.18.0", "2.1"),
        ];

        let resolution = resolve(&recorded, &[], &releases, FACTORIO);

        assert_eq!(
            (resolution.swaps.clone(), downloads(&resolution)),
            (vec![], vec!["flib 0.17.2".to_string()])
        );
    }
}
