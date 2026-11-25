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

/// Files directly in `dir` whose name matches `pattern`, sorted by name. Matching nothing is
/// an error, so a typo never yields an empty run.
pub fn select(dir: &Path, pattern: &str, kind: Kind) -> Result<Vec<PathBuf>, SelectError> {
    let glob = glob::Pattern::new(pattern).map_err(|source| SelectError::Pattern {
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
    fn a_pattern_matching_nothing_is_an_error() {
        let dir = dir_with(&["base-a.zip"]);

        let result = select(dir.path(), "nope-*", Kind::Saves);

        assert!(matches!(result, Err(SelectError::NothingMatched { .. })));
    }
}
