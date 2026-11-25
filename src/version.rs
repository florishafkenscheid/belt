//! `major.minor.patch` versions, as Factorio writes them for itself and for mods.

use std::fmt;

/// A Factorio or mod version. Ordered numerically, so `1.300.0` is newer than `1.30.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parses `major.minor.patch`; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('.').map(|part| part.parse::<u32>().ok());
        let version = Self::new(parts.next()??, parts.next()??, parts.next()??);
        if parts.next().is_some() {
            return None;
        }
        Some(version)
    }

    /// The `major.minor` a mod's `info.json` names as its `factorio_version`.
    pub fn family(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically_not_as_text() {
        assert!(Version::parse("1.300.0") > Version::parse("1.30.0"));
    }

    #[test]
    fn a_version_needs_exactly_three_numbers() {
        assert_eq!(
            [
                Version::parse("2.1"),
                Version::parse("2.1.0.1"),
                Version::parse("2.x.0")
            ],
            [None, None, None]
        );
    }
}
