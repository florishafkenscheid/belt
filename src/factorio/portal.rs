//! The Factorio mod portal: which releases a mod has (public), and downloading one (with the
//! user's portal token). Used for recorded mod versions installed nowhere, and for the
//! belt-sanitizer release itself.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha1::{Digest, Sha1};

use super::mods::Credentials;
use crate::version::Version;

const OFFICIAL: &str = "https://mods.factorio.com";
/// The largest mod download accepted.
const MAX_DOWNLOAD: u64 = 1 << 30;

/// One release of a mod on the portal.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub name: String,
    pub version: Version,
    /// The Factorio `major.minor` it is made for.
    pub factorio_version: String,
    download_url: String,
    file_name: String,
    sha1: String,
}

/// A release for tests elsewhere in the adapter.
#[cfg(test)]
pub fn release(name: &str, version: &str, factorio_version: &str) -> Release {
    Release {
        name: name.into(),
        version: Version::parse(version).unwrap(),
        factorio_version: factorio_version.into(),
        download_url: format!("/download/{name}/{version}"),
        file_name: format!("{name}_{version}.zip"),
        sha1: String::new(),
    }
}

#[derive(Deserialize)]
struct ModInfo {
    name: String,
    releases: Vec<WireRelease>,
}

#[derive(Deserialize)]
struct WireRelease {
    version: String,
    download_url: String,
    file_name: String,
    sha1: String,
    info_json: WireInfo,
}

#[derive(Deserialize)]
struct WireInfo {
    factorio_version: String,
}

/// The releases in the portal's `/api/mods/<name>/full` answer.
fn parse_releases(json: &str) -> Result<Vec<Release>, String> {
    let info: ModInfo = serde_json::from_str(json)
        .map_err(|error| format!("unexpected mod portal answer: {error}"))?;
    info.releases
        .into_iter()
        .map(|release| {
            let version = Version::parse(&release.version).ok_or_else(|| {
                format!(
                    "the mod portal lists {} version {:?}",
                    info.name, release.version
                )
            })?;
            Ok(Release {
                name: info.name.clone(),
                version,
                factorio_version: release.info_json.factorio_version,
                download_url: release.download_url,
                file_name: release.file_name,
                sha1: release.sha1,
            })
        })
        .collect()
}

/// A mod portal, by its base URL.
#[derive(Debug, Clone)]
pub struct Portal {
    base: String,
}

impl Portal {
    /// mods.factorio.com.
    pub fn official() -> Self {
        Self {
            base: OFFICIAL.into(),
        }
    }

    /// A portal at another address (tests run a local one).
    #[cfg(test)]
    pub fn at(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').into(),
        }
    }

    /// Every release of `name` on the portal; a mod the portal doesn't know has none.
    pub fn releases(&self, name: &str) -> Result<Vec<Release>, String> {
        let url = format!("{}/api/mods/{name}/full", self.base);
        match ureq::get(&url).call() {
            Ok(mut response) => {
                let json = response
                    .body_mut()
                    .read_to_string()
                    .map_err(|error| format!("{url}: {error}"))?;
                parse_releases(&json)
            }
            Err(ureq::Error::StatusCode(404)) => Ok(Vec::new()),
            Err(error) => Err(format!("{url}: {error}")),
        }
    }

    /// Downloads `release` into `dir` as the portal names the file, checking its SHA-1.
    pub fn download(
        &self,
        release: &Release,
        credentials: &Credentials,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        let url = format!("{}{}", self.base, release.download_url);
        let failed = |error: String| {
            format!(
                "downloading {} {} from the mod portal: {error}",
                release.name, release.version
            )
        };
        let mut response = ureq::get(&url)
            .query("username", &credentials.username)
            .query("token", &credentials.token)
            .call()
            .map_err(|error| failed(error.to_string()))?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD)
            .read_to_vec()
            .map_err(|error| failed(error.to_string()))?;
        check_sha1(&bytes, &release.sha1).map_err(failed)?;
        let path = dir.join(&release.file_name);
        std::fs::write(&path, bytes)
            .map_err(|error| failed(format!("{}: {error}", path.display())))?;
        Ok(path)
    }
}

impl Release {
    /// Where the release lives in `dir` once downloaded.
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(&self.file_name)
    }

    /// Whether `path` holds exactly this release (its SHA-1 matches the portal's).
    pub fn is_downloaded_at(&self, path: &Path) -> bool {
        std::fs::read(path).is_ok_and(|bytes| check_sha1(&bytes, &self.sha1).is_ok())
    }
}

fn check_sha1(bytes: &[u8], expected: &str) -> Result<(), String> {
    let found: String = Sha1::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if found != expected.to_lowercase() {
        return Err(format!(
            "SHA-1 {found} doesn't match the portal's {expected}"
        ));
    }
    Ok(())
}

/// A local stand-in for the mod portal, for tests: it answers each request whose path starts
/// with one of its routes with that route's body, and 404 otherwise.
#[cfg(test)]
pub mod fake {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    pub struct FakePortal {
        pub base: String,
    }

    impl FakePortal {
        pub fn serve(routes: Vec<(String, Vec<u8>)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { return };
                    let mut request_line = String::new();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    reader.read_line(&mut request_line).unwrap();
                    let mut header = String::new();
                    while reader.read_line(&mut header).is_ok_and(|read| read > 2) {
                        header.clear();
                    }
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string();
                    let found = routes
                        .iter()
                        .find(|(route, _)| path.starts_with(route.as_str()));
                    let (status, body) = match found {
                        Some((_, body)) => ("200 OK", body.clone()),
                        None => ("404 Not Found", Vec::new()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(head.as_bytes()).unwrap();
                    stream.write_all(&body).unwrap();
                }
            });
            Self { base }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_carry_the_factorio_version_they_are_made_for() {
        let json = r#"{"name": "RateCalculator", "releases": [
            {"version": "3.3.7", "download_url": "/download/RateCalculator/a", "file_name": "RateCalculator_3.3.7.zip",
             "sha1": "00", "info_json": {"factorio_version": "2.0"}}]}"#;

        let releases = parse_releases(json).unwrap();

        assert_eq!(
            (releases[0].version, releases[0].factorio_version.as_str()),
            (Version::new(3, 3, 7), "2.0")
        );
    }

    #[test]
    fn a_download_whose_sha1_differs_from_the_portal_is_refused() {
        // SHA-1 of "abc".
        let digest = "a9993e364706816aba3e25717850c26c9cd0d89d";

        assert_eq!(
            [
                check_sha1(b"abc", digest).is_ok(),
                check_sha1(b"abd", digest).is_ok()
            ],
            [true, false]
        );
    }
}
