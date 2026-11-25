//! Selecting input files: saves for benchmark and sanitize, blueprint files for blueprint.

use std::path::{Path, PathBuf};

/// A Factorio save file. `name` is its file stem, as reported everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Save {
    pub path: PathBuf,
    pub name: String,
}

impl Save {
    pub fn new(path: PathBuf) -> Self {
        let name = file_stem(&path);
        Self { path, name }
    }
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum SelectError {
    #[error("invalid pattern {pattern:?}: {source}")]
    Pattern {
        pattern: String,
        source: glob::PatternError,
    },
    #[error("cannot read {dir}: {source}")]
    Read {
        dir: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a save: saves are .zip files")]
    NotASave { path: PathBuf },
    #[error("no {what} in {dir} match {pattern:?}")]
    NothingMatched {
        what: &'static str,
        dir: PathBuf,
        pattern: String,
    },
}

/// Which files count, before the user's pattern is applied.
#[derive(Clone, Copy)]
pub enum Kind {
    /// `.zip` files.
    Saves,
    /// Any file.
    Blueprints,
}

impl Kind {
    /// The glob a file name is matched against. A save pattern names saves without their
    /// `.zip`, as 4.3.1 globbed `<dir>/<pattern>.zip`: `CHOAM` selects `CHOAM.zip`.
    fn glob(self, pattern: &str) -> String {
        match self {
            Kind::Saves => format!("{pattern}.zip"),
            Kind::Blueprints => pattern.to_string(),
        }
    }

    fn admits(self, path: &Path) -> bool {
        match self {
            Kind::Saves => path.extension().is_some_and(|extension| extension == "zip"),
            Kind::Blueprints => true,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Kind::Saves => "saves",
            Kind::Blueprints => "blueprint files",
        }
    }
}

/// The files `path` names: `path` itself when it is a file (the pattern doesn't apply; a save
/// must still be a `.zip`), or else the files directly in that directory that `pattern` matches
/// (see [`Kind::glob`]), sorted by name. Matching nothing is an error, so a typo never yields an empty run.
pub fn select(path: &Path, pattern: &str, kind: Kind) -> Result<Vec<PathBuf>, SelectError> {
    if path.is_file() {
        if !kind.admits(path) {
            return Err(SelectError::NotASave { path: path.into() });
        }
        return Ok(vec![path.into()]);
    }
    let dir = path;
    let glob = glob::Pattern::new(&kind.glob(pattern)).map_err(|source| SelectError::Pattern {
        pattern: pattern.into(),
        source,
    })?;
    let entries = std::fs::read_dir(dir).map_err(|source| SelectError::Read {
        dir: dir.into(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| SelectError::Read {
                dir: dir.into(),
                source,
            })?
            .path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_file() && kind.admits(&path) && glob.matches(&name) {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Err(SelectError::NothingMatched {
            what: kind.noun(),
            dir: dir.into(),
            pattern: pattern.into(),
        });
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        dir
    }

    fn names(paths: Vec<PathBuf>) -> Vec<String> {
        paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_pattern_selects_saves_by_file_name() {
        let dir = dir_with(&["base-a.zip", "base-b.zip", "other.zip", "base-c.txt"]);

        let selected = select(dir.path(), "base-*", Kind::Saves).unwrap();

        assert_eq!(names(selected), vec!["base-a.zip", "base-b.zip"]);
    }

    #[test]
    fn a_save_pattern_matches_the_name_without_zip() {
        let dir = dir_with(&["CHOAM.zip", "CHOAM-old.zip"]);

        let selected = select(dir.path(), "CHOAM", Kind::Saves).unwrap();

        assert_eq!(names(selected), vec!["CHOAM.zip"]);
    }

    #[test]
    fn a_save_pattern_can_end_in_a_wildcard_before_the_zip() {
        let dir = dir_with(&["clean_car.zip", "dirty_car.zip", "car_park.zip"]);

        let selected = select(dir.path(), "*car", Kind::Saves).unwrap();

        assert_eq!(names(selected), vec!["clean_car.zip", "dirty_car.zip"]);
    }

    #[test]
    fn a_blueprint_pattern_matches_the_whole_file_name() {
        let dir = dir_with(&["design_07.txt", "design_08.txt"]);

        let selected = select(dir.path(), "*07*", Kind::Blueprints).unwrap();

        assert_eq!(names(selected), vec!["design_07.txt"]);
    }

    #[test]
    fn a_save_file_is_selected_on_its_own() {
        let dir = dir_with(&["base-a.zip", "base-b.zip"]);

        let selected = select(&dir.path().join("base-a.zip"), "other-*", Kind::Saves).unwrap();

        assert_eq!(names(selected), vec!["base-a.zip"]);
    }

    #[test]
    fn a_file_that_is_not_a_zip_is_not_a_save() {
        let dir = dir_with(&["notes.txt"]);

        let result = select(&dir.path().join("notes.txt"), "*", Kind::Saves);

        assert!(matches!(result, Err(SelectError::NotASave { .. })));
    }

    #[test]
    fn a_blueprint_file_is_selected_on_its_own() {
        let dir = dir_with(&["design.txt"]);

        let selected = select(&dir.path().join("design.txt"), "*", Kind::Blueprints).unwrap();

        assert_eq!(names(selected), vec!["design.txt"]);
    }

    #[test]
    fn a_pattern_matching_nothing_is_an_error() {
        let dir = dir_with(&["base-a.zip"]);

        let result = select(dir.path(), "nope-*", Kind::Saves);

        assert!(matches!(result, Err(SelectError::NothingMatched { .. })));
    }
}
