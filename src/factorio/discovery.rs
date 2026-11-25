//! Finding the Factorio binary and Factorio's user data directory.
//!
//! The OS is a runtime value (`std::env::consts::OS`), so every OS's lists are plain data that
//! tests can exercise on any machine against a fake directory tree.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DiscoveryError {
    #[error("Factorio not found at {0}")]
    ExplicitMissing(PathBuf),
    #[error(
        "Factorio not found in the usual install locations; pass its binary with --factorio (or set BELT_FACTORIO_EXECUTABLE or [factorio] executable)"
    )]
    NotFound,
    #[error("Factorio's user data directory is unknown on {0}; set it with --factorio-data")]
    UnknownDataDir(String),
}

/// Where the machine keeps things, so tests can point it at a fake tree.
pub struct Places<'a> {
    pub os: &'a str,
    pub home: &'a Path,
    /// `/` on Unix, the system drive on Windows.
    pub root: &'a Path,
    /// Windows' roaming application data directory.
    pub roaming_app_data: &'a Path,
}

/// Steam, standalone and headless install locations, most common first.
fn candidates(places: &Places) -> Vec<PathBuf> {
    let home = places.home;
    let root = places.root;
    let linux_binary = "bin/x64/factorio";
    let mac_binary = "factorio.app/Contents/MacOS/factorio";
    let windows_binary = "bin/x64/factorio.exe";
    match places.os {
        "linux" => vec![
            home.join(".steam/steam/steamapps/common/Factorio")
                .join(linux_binary),
            home.join(".local/share/Steam/steamapps/common/Factorio")
                .join(linux_binary),
            home.join(
                ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/Factorio",
            )
            .join(linux_binary),
            home.join("factorio").join(linux_binary),
            home.join(".factorio").join(linux_binary),
            root.join("opt/factorio").join(linux_binary),
        ],
        "macos" => vec![
            home.join("Library/Application Support/Steam/steamapps/common/Factorio")
                .join(mac_binary),
            root.join("Applications").join(mac_binary),
            home.join("Applications").join(mac_binary),
        ],
        "windows" => vec![
            root.join("Program Files (x86)/Steam/steamapps/common/Factorio")
                .join(windows_binary),
            root.join("Program Files/Steam/steamapps/common/Factorio")
                .join(windows_binary),
            root.join("Program Files/Factorio").join(windows_binary),
        ],
        _ => Vec::new(),
    }
}

/// The binary to run: the explicit path when given, otherwise the first install found.
pub fn find_factorio(explicit: Option<&Path>, places: &Places) -> Result<PathBuf, DiscoveryError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(DiscoveryError::ExplicitMissing(path.to_path_buf()));
        }
        return Ok(path.to_path_buf());
    }
    candidates(places)
        .into_iter()
        .find(|path| path.is_file())
        .ok_or(DiscoveryError::NotFound)
}

/// Factorio's default user data directory (saves, mods, `player-data.json`) on this OS.
pub fn default_data_dir(places: &Places) -> Result<PathBuf, DiscoveryError> {
    match places.os {
        "linux" => Ok(places.home.join(".factorio")),
        "macos" => Ok(places.home.join("Library/Application Support/factorio")),
        "windows" => Ok(places.roaming_app_data.join("Factorio")),
        other => Err(DiscoveryError::UnknownDataDir(other.into())),
    }
}

/// The install's read-only data directory, found next to the binary: `bin/x64/factorio` on
/// Linux and Windows, `Contents/MacOS/factorio` on macOS.
pub fn install_data_dir(executable: &Path) -> Option<PathBuf> {
    let binary_dir = executable.parent()?;
    [binary_dir.join("../../data"), binary_dir.join("../data")]
        .into_iter()
        .find(|dir| dir.join("base/info.json").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree {
        dir: tempfile::TempDir,
    }

    impl Tree {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().unwrap(),
            }
        }

        fn home(&self) -> PathBuf {
            self.dir.path().join("home")
        }

        fn root(&self) -> PathBuf {
            self.dir.path().join("root")
        }

        fn touch(&self, path: &Path) {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }

        fn find(&self, os: &str, explicit: Option<&Path>) -> Result<PathBuf, DiscoveryError> {
            let (home, root) = (self.home(), self.root());
            find_factorio(
                explicit,
                &Places {
                    os,
                    home: &home,
                    root: &root,
                    roaming_app_data: &home,
                },
            )
        }
    }

    #[test]
    fn an_explicit_factorio_path_is_used_as_given() {
        let tree = Tree::new();
        let steam = tree
            .home()
            .join(".steam/steam/steamapps/common/Factorio/bin/x64/factorio");
        let explicit = tree.root().join("custom/factorio");
        tree.touch(&steam);
        tree.touch(&explicit);

        assert_eq!(tree.find("linux", Some(&explicit)), Ok(explicit));
    }

    #[test]
    fn no_factorio_found_is_an_error_naming_the_option() {
        let tree = Tree::new();

        let error = tree.find("linux", None).unwrap_err();

        assert!(error.to_string().contains("--factorio"), "{error}");
    }

    #[test]
    fn a_steam_install_is_found_on_linux() {
        let tree = Tree::new();
        let steam = tree
            .home()
            .join(".local/share/Steam/steamapps/common/Factorio/bin/x64/factorio");
        tree.touch(&steam);

        assert_eq!(tree.find("linux", None), Ok(steam));
    }

    #[test]
    fn a_headless_install_in_opt_is_found_on_linux() {
        let tree = Tree::new();
        let headless = tree.root().join("opt/factorio/bin/x64/factorio");
        tree.touch(&headless);

        assert_eq!(tree.find("linux", None), Ok(headless));
    }

    #[test]
    fn the_standalone_app_is_found_on_macos() {
        let tree = Tree::new();
        let app = tree
            .root()
            .join("Applications/factorio.app/Contents/MacOS/factorio");
        tree.touch(&app);

        assert_eq!(tree.find("macos", None), Ok(app));
    }

    #[test]
    fn the_standalone_install_is_found_on_windows() {
        let tree = Tree::new();
        let standalone = tree
            .root()
            .join("Program Files/Factorio/bin/x64/factorio.exe");
        tree.touch(&standalone);

        assert_eq!(tree.find("windows", None), Ok(standalone));
    }

    #[test]
    fn steam_wins_over_the_standalone_install() {
        let tree = Tree::new();
        let steam = tree
            .root()
            .join("Program Files (x86)/Steam/steamapps/common/Factorio/bin/x64/factorio.exe");
        tree.touch(&steam);
        tree.touch(
            &tree
                .root()
                .join("Program Files/Factorio/bin/x64/factorio.exe"),
        );

        assert_eq!(tree.find("windows", None), Ok(steam));
    }
}
