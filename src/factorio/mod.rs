//! The Factorio adapter: implements the benchmark, blueprint and sanitize ports by running the
//! real game, isolated in a run directory with B.E.L.T.'s mod cache.

pub mod discovery;
pub mod launch;
mod mods;
pub mod portal;
mod process;
pub mod workspace;

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::benchmark::parse::{BenchmarkParser, Measurement};
use crate::benchmark::{BenchmarkRequest, BenchmarkRunner};
use crate::blueprint::Stamper;
use crate::failure::{FailureReason, RunFailure};
use crate::protocol::{
    self, Job, Line, Message, SanitizeJob, SanitizeReport, StampJob, StampReport,
};
use crate::sanitize::Inspector;
use crate::saves::Save;
use crate::version::Version;
pub use mods::ModsError;
use mods::{ModSet, Sanitizer};
use portal::Portal;
use process::{Finished, Supervisor};
use workspace::{RunDir, Workspace};

/// Ticks a sanitize run may take beyond the job's before Factorio ends it, so a mod that never
/// answers ends as "no result" instead of running forever.
const SANITIZE_GRACE_TICKS: u32 = 60;

/// Where a save's mods come from.
#[derive(Debug, Clone)]
pub enum ModSource {
    /// The mods the save recorded, synced by Factorio. Mods are copied from `user_mods`.
    Recorded { user_mods: PathBuf },
    /// Every mod in this directory, for every save.
    Directory(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum FactorioError {
    #[error("no Factorio data directory next to {0}")]
    NoInstallData(PathBuf),
    #[error(transparent)]
    Mods(#[from] ModsError),
    #[error(transparent)]
    Workspace(#[from] workspace::WorkspaceError),
    #[error("{0}")]
    Supervisor(String),
    #[error("cannot read Factorio's version from {executable}: {message}")]
    Version {
        executable: PathBuf,
        message: String,
    },
    #[error(transparent)]
    TooOld(#[from] launch::TooOld),
    #[error("{0}")]
    Sanitizer(String),
}

/// Where the belt-sanitizer mod comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum SanitizerMod {
    /// The command doesn't run the mod.
    NotNeeded,
    /// A mod folder, for development.
    Folder(PathBuf),
    /// The mod portal's newest release for this CLI's interface version and this Factorio.
    Portal,
}

/// Everything [`Factorio::new`] needs.
pub struct FactorioOptions {
    pub executable: PathBuf,
    pub workspace: Workspace,
    pub mods: ModSource,
    /// Factorio's `player-data.json`, for the mod portal token when a download is needed.
    pub player_data: PathBuf,
    pub sanitizer: SanitizerMod,
    pub portal: Portal,
    pub command: launch::Command,
    pub verbose: bool,
}

pub struct Factorio {
    executable: PathBuf,
    install_data: PathBuf,
    workspace: Workspace,
    cache: PathBuf,
    builtins: BTreeSet<String>,
    source: ModSource,
    /// The belt-sanitizer mod, already checked against the interface version.
    sanitizer: Option<mods::SanitizerSource>,
    player_data: PathBuf,
    portal: Portal,
    supervisor: Supervisor,
    resolved: HashMap<PathBuf, ModSet>,
    /// The running Factorio's version, read before anything else.
    version: Version,
}

impl Factorio {
    /// Reads Factorio's version and refuses one too old for the command, then readies the
    /// belt-sanitizer mod (checking or downloading it) when the command needs it.
    pub fn new(options: FactorioOptions) -> Result<Self, FactorioError> {
        let FactorioOptions {
            executable,
            workspace,
            mods: source,
            player_data,
            sanitizer,
            portal,
            command,
            verbose,
        } = options;
        let install_data = discovery::install_data_dir(&executable)
            .ok_or_else(|| FactorioError::NoInstallData(executable.clone()))?;
        let version = read_version(&executable, &install_data, &workspace)?;
        launch::check(version, command)?;
        let cache = workspace.mod_cache();
        std::fs::create_dir_all(&cache).map_err(|source| ModsError::Io {
            path: cache.clone(),
            source,
        })?;
        let sanitizer = match sanitizer {
            SanitizerMod::NotNeeded => None,
            SanitizerMod::Folder(folder) => Some(mods::SanitizerSource::Folder(folder)),
            SanitizerMod::Portal => Some(fetch_sanitizer(
                &portal,
                &workspace.downloads(),
                &player_data,
                version,
            )?),
        };
        if let Some(sanitizer) = &sanitizer {
            mods::check_sanitizer(sanitizer)?;
        }
        let echo = if verbose {
            process::Echo::Verbose
        } else {
            process::Echo::Quiet
        };
        Ok(Self {
            builtins: mods::builtins(&install_data)?,
            executable,
            install_data,
            workspace,
            cache,
            source,
            sanitizer,
            player_data,
            portal,
            supervisor: Supervisor::new(echo).map_err(FactorioError::Supervisor)?,
            resolved: HashMap::new(),
            version,
        })
    }

    fn new_run(&self) -> Result<RunDir, RunFailure> {
        self.workspace.new_run().map_err(setup)
    }

    /// Runs Factorio in `run` with B.E.L.T.'s config and mod cache.
    fn spawn(
        &self,
        run: &RunDir,
        args: &[OsString],
        on_line: impl FnMut(&str, &mut process::Stdin),
    ) -> Result<Finished, RunFailure> {
        let config = write_config(run, &self.install_data).map_err(setup)?;
        let mut command = Command::new(&self.executable);
        command
            .arg("-c")
            .arg(&config)
            .arg("--mod-directory")
            .arg(&self.cache)
            .args(args);
        self.supervisor.run(command, run, on_line)
    }

    /// The mods `save` runs with, resolved once per save.
    fn mods_for(&mut self, save: &Path) -> Result<ModSet, RunFailure> {
        if let Some(set) = self.resolved.get(save) {
            return Ok(set.clone());
        }
        let set = match self.source.clone() {
            ModSource::Directory(dir) => {
                mods::from_directory(&dir, &self.cache, &self.builtins).map_err(setup)?
            }
            ModSource::Recorded { user_mods } => self.recorded_mods(save, &user_mods)?,
        };
        self.resolved.insert(save.to_path_buf(), set.clone());
        Ok(set)
    }

    /// A save's recorded mods, as Factorio's sync lists them, resolved against what is
    /// installed and, for versions installed nowhere, what the mod portal has (see
    /// [`mods::resolve`]). Downloads use the user's portal token. Every swap of a version made for
    /// another Factorio is a warning; a mod with no version that loads fails the save.
    fn recorded_mods(&self, save: &Path, user_mods: &Path) -> Result<ModSet, RunFailure> {
        let entries = self.sync(save)?;
        let mut set =
            mods::needed(&entries, &self.cached()?, &self.builtins).map_err(mods_failure)?;
        let available = self.available(user_mods)?;
        let mut resolution = mods::resolve(&set.mods, &available, &[], self.version);
        if !resolution.missing.is_empty() {
            let mut releases = Vec::new();
            for missing in &resolution.missing {
                releases.extend(self.portal.releases(&missing.name).map_err(mods_failure)?);
            }
            resolution = mods::resolve(&set.mods, &available, &releases, self.version);
        }
        if !resolution.missing.is_empty() {
            let list = join(&resolution.missing);
            return Err(mods_failure(format!(
                "{list} isn't installed, and the mod portal doesn't have it"
            )));
        }
        if !resolution.unloadable.is_empty() {
            let list: Vec<String> = resolution
                .unloadable
                .iter()
                .map(ToString::to_string)
                .collect();
            let family = self.version.family();
            return Err(mods_failure(format!(
                "{} can't load on Factorio {family}, and no version of it that can is installed or on the mod portal",
                list.join(", ")
            )));
        }
        if !resolution.downloads.is_empty() {
            let list: Vec<String> = resolution
                .downloads
                .iter()
                .map(|release| format!("{} {}", release.name, release.version))
                .collect();
            let credentials = mods::read_credentials(&self.player_data).map_err(|error| {
                mods_failure(format!(
                    "{} must be downloaded from the mod portal, but there is {error}",
                    list.join(", ")
                ))
            })?;
            for release in &resolution.downloads {
                let path = self
                    .portal
                    .download(release, &credentials, &self.cache)
                    .map_err(mods_failure)?;
                eprintln!("downloaded {}", path.display());
            }
        }
        let cached = self.cached()?;
        for chosen in &resolution.chosen {
            let in_cache = cached.iter().any(|found| &found.mod_ref == chosen);
            if !in_cache && !mods::copy_into_cache(chosen, user_mods, &self.cache).map_err(setup)? {
                return Err(setup(format!(
                    "{chosen} vanished from {} while syncing",
                    user_mods.display()
                )));
            }
        }
        let name = crate::saves::file_stem(save);
        for swap in &resolution.swaps {
            eprintln!("WARNING {name}: {swap}");
        }
        set.mods = resolution.chosen;
        set.settings = mods::read_settings(&self.cache).map_err(setup)?;
        Ok(set)
    }

    /// Every installed mod a save could use: the cache's, then the user's.
    fn available(&self, user_mods: &Path) -> Result<Vec<mods::InstalledMod>, RunFailure> {
        let mut available = self.cached()?;
        if user_mods.is_dir() {
            available.extend(mods::installed(user_mods).map_err(setup)?);
        }
        Ok(available)
    }

    fn cached(&self) -> Result<Vec<mods::InstalledMod>, RunFailure> {
        mods::installed(&self.cache).map_err(setup)
    }

    /// `--sync-mods`: Factorio writes the save's recorded mods into the cache's `mod-list.json`
    /// (and its startup settings into `mod-settings.dat`). Downloads are B.E.L.T.'s own (see
    /// `portal`): with a token, Factorio 2.1.19's sync still downloaded nothing (research.md).
    fn sync(&self, save: &Path) -> Result<Vec<mods::ListEntry>, RunFailure> {
        let run = self.new_run()?;
        let finished = self.spawn(
            &run,
            &[OsString::from("--sync-mods"), save.into()],
            |_, _| {},
        )?;
        if finished.interrupted {
            return Err(RunFailure::new(FailureReason::Interrupted));
        }
        if finished.code != Some(0) {
            let reason = format!(
                "Factorio could not sync the save's mods (exit code {:?})",
                finished.code
            );
            return Err(RunFailure::with_tail(
                FailureReason::Mods(reason),
                finished.tail,
            ));
        }
        mods::read_mod_list(&self.cache).map_err(setup)
    }

    fn sanitizer(&self) -> Result<&mods::SanitizerSource, RunFailure> {
        self.sanitizer
            .as_ref()
            .ok_or_else(|| setup("this command doesn't run the belt-sanitizer mod"))
    }

    /// Runs the mod with `job` and collects its messages. With [`Ending::Quit`], the server is
    /// told to `/quit` (and so to save) once the mod has answered.
    fn run_mod(
        &mut self,
        save: &Path,
        run: &RunDir,
        job: &Job,
        args: &[OsString],
        ending: Ending,
    ) -> Result<(ModRun, Finished), RunFailure> {
        let set = self.mods_for(save)?;
        mods::install_sanitizer(self.sanitizer()?, &self.cache, job).map_err(setup)?;
        mods::prepare_run(&self.cache, &self.builtins, &set, Sanitizer::Enabled).map_err(setup)?;
        let mut mod_run = ModRun::default();
        let mut quit_sent = false;
        let finished = self.spawn(run, args, |line, stdin| {
            let answered = mod_run.line(line);
            if answered && ending == Ending::Quit && !quit_sent {
                quit_sent = true;
                if let Err(error) = stdin.send("/quit") {
                    eprintln!("cannot tell Factorio to quit: {error}");
                }
            }
        })?;
        Ok((mod_run, finished))
    }
}

/// Writes the run's `config.ini`: the install's data, and write-data inside the run directory,
/// so Factorio never writes to its own user data directory.
fn write_config(run: &RunDir, install_data: &Path) -> std::io::Result<PathBuf> {
    let write_data = run.path().join("data");
    std::fs::create_dir_all(write_data.join("saves"))?;
    let config = run.path().join("config.ini");
    let ini = format!(
        "[path]\nread-data={}\nwrite-data={}\n",
        install_data.display(),
        write_data.display()
    );
    std::fs::write(&config, ini)?;
    Ok(config)
}

/// `factorio --version`, run with a run directory's config like every other launch.
fn read_version(
    executable: &Path,
    install_data: &Path,
    workspace: &Workspace,
) -> Result<Version, FactorioError> {
    let fail = |message: String| FactorioError::Version {
        executable: executable.to_path_buf(),
        message,
    };
    let run = workspace.new_run()?;
    let config = write_config(&run, install_data).map_err(|error| fail(error.to_string()))?;
    let output = Command::new(executable)
        .arg("-c")
        .arg(&config)
        .arg("--version")
        .output()
        .map_err(|error| fail(error.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    launch::parse_version(&stdout)
        .ok_or_else(|| fail(format!("unexpected output {:?}", stdout.trim())))
}

/// One benchmark run (`--benchmark-runs 1`, so the run order can interleave saves). Verbose
/// timings are always asked for as `all`: on 2.1.19 the full build crashes at the end of a
/// benchmark when `--benchmark-verbose` is a list of names, never with `all`.
fn benchmark_args(save: &Save, request: &BenchmarkRequest) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "--benchmark".into(),
        save.path.clone().into(),
        "--benchmark-ticks".into(),
        request.ticks.to_string().into(),
        "--benchmark-runs".into(),
        "1".into(),
    ];
    if request.verbose {
        args.extend(["--benchmark-verbose".into(), "all".into()]);
    }
    args
}

/// The mod portal's newest belt-sanitizer release for this CLI's interface version that loads
/// on `factorio`, downloaded into `downloads` unless it is already there. (Not into the mod
/// cache: Factorio would see the zip and the installed folder as two copies of the mod.)
fn fetch_sanitizer(
    portal: &Portal,
    downloads: &Path,
    player_data: &Path,
    factorio: Version,
) -> Result<mods::SanitizerSource, FactorioError> {
    let failed = |message: String| FactorioError::Sanitizer(message);
    let releases = portal.releases(protocol::MOD_NAME).map_err(failed)?;
    let Some(release) = releases
        .iter()
        .filter(|release| {
            release.version.major == protocol::INTERFACE
                && release.factorio_version == factorio.family()
        })
        .max_by_key(|release| release.version)
    else {
        return Err(failed(format!(
            "the mod portal has no {} {}.x release for Factorio {}; pass --sanitizer-mod to use a local copy",
            protocol::MOD_NAME,
            protocol::INTERFACE,
            factorio.family()
        )));
    };
    std::fs::create_dir_all(downloads)
        .map_err(|error| failed(format!("{}: {error}", downloads.display())))?;
    let path = release.path_in(downloads);
    if !release.is_downloaded_at(&path) {
        let credentials = mods::read_credentials(player_data).map_err(|error| {
            failed(format!(
                "{} {} must be downloaded from the mod portal, but there is {error}",
                release.name, release.version
            ))
        })?;
        portal
            .download(release, &credentials, downloads)
            .map_err(failed)?;
        eprintln!("downloaded {} {}", release.name, release.version);
    }
    Ok(mods::SanitizerSource::Zip(path))
}

/// How a mod run ends once the mod has answered.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ending {
    /// The mod ends the run itself (`--test-save` and `script.finish_test`).
    ByMod,
    /// The CLI sends `/quit` to the server, which then saves the map.
    Quit,
}

fn setup(error: impl std::fmt::Display) -> RunFailure {
    RunFailure::new(FailureReason::Setup(error.to_string()))
}

fn mods_failure(message: String) -> RunFailure {
    RunFailure::new(FailureReason::Mods(message))
}

fn join(mods: &[mods::ModRef]) -> String {
    mods.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Everything the mod said during one run.
#[derive(Debug, Default)]
struct ModRun {
    interface: Option<u32>,
    error: Option<String>,
    protocol: Option<String>,
    sanitize: Option<SanitizeReport>,
    stamp: Option<StampReport>,
}

impl ModRun {
    /// Reads one stdout line. Returns true when the mod has answered (result or error).
    fn line(&mut self, line: &str) -> bool {
        match protocol::parse_line(line) {
            Ok(Line::Factorio) => false,
            Ok(Line::Message(Message::Hello { interface })) => {
                self.interface = Some(interface);
                false
            }
            Ok(Line::Message(Message::Warning { text })) => {
                eprintln!("WARNING {text}");
                false
            }
            Ok(Line::Message(Message::Error { message })) => {
                self.error = Some(message);
                true
            }
            Ok(Line::Message(Message::SanitizeResult { report })) => {
                self.sanitize = Some(report);
                true
            }
            Ok(Line::Message(Message::StampResult { report })) => {
                self.stamp = Some(report);
                true
            }
            Err(error) => {
                self.protocol = Some(error.to_string());
                true
            }
        }
    }

    /// The run's answer: `result` only if the mod spoke our interface, reported no error, and
    /// Factorio exited 0.
    fn conclude<T>(&self, result: Option<T>, finished: Finished) -> Result<T, RunFailure> {
        let fail = |reason| Err(RunFailure::with_tail(reason, finished.tail.clone()));
        if finished.interrupted {
            return fail(FailureReason::Interrupted);
        }
        if let Some(message) = &self.protocol {
            return fail(FailureReason::Protocol(message.clone()));
        }
        if let Some(message) = &self.error {
            return fail(FailureReason::Mod(message.clone()));
        }
        if let Some(interface) = self.interface
            && interface != protocol::INTERFACE
        {
            return fail(FailureReason::Protocol(format!(
                "{} speaks interface {interface}, this belt needs {}",
                protocol::MOD_NAME,
                protocol::INTERFACE
            )));
        }
        if finished.code != Some(0) {
            return fail(FailureReason::Exited(finished.code));
        }
        match result {
            Some(result) => Ok(result),
            None => fail(FailureReason::NoResult),
        }
    }
}

impl BenchmarkRunner for Factorio {
    fn prepare(&mut self, save: &Save) -> Result<(), RunFailure> {
        self.mods_for(&save.path).map(|_| ())
    }

    fn benchmark(
        &mut self,
        save: &Save,
        request: &BenchmarkRequest,
    ) -> Result<Measurement, RunFailure> {
        let set = self.mods_for(&save.path)?;
        mods::prepare_run(&self.cache, &self.builtins, &set, Sanitizer::Disabled).map_err(setup)?;
        let args = benchmark_args(save, request);
        let run = self.new_run()?;
        let mut parser = BenchmarkParser::default();
        let finished = self.spawn(&run, &args, |line, _| parser.line(line))?;
        if finished.interrupted {
            return Err(RunFailure::with_tail(
                FailureReason::Interrupted,
                finished.tail,
            ));
        }
        if finished.code != Some(0) {
            return Err(RunFailure::with_tail(
                FailureReason::Exited(finished.code),
                finished.tail,
            ));
        }
        parser
            .finish()
            .map_err(|message| RunFailure::with_tail(FailureReason::Output(message), finished.tail))
    }
}

impl Inspector for Factorio {
    fn sanitize(&mut self, save: &Save, job: &SanitizeJob) -> Result<SanitizeReport, RunFailure> {
        let args: Vec<OsString> = vec![
            "--test-save".into(),
            save.path.clone().into(),
            "--test-ticks".into(),
            (job.ticks + SANITIZE_GRACE_TICKS).to_string().into(),
        ];
        let run = self.new_run()?;
        let (mod_run, finished) = self.run_mod(
            &save.path,
            &run,
            &Job::Sanitize(job.clone()),
            &args,
            Ending::ByMod,
        )?;
        let report = mod_run.sanitize.clone();
        mod_run.conclude(report, finished)
    }
}

impl Stamper for Factorio {
    fn stamp(
        &mut self,
        base: &Path,
        job: &StampJob,
        output: &Path,
    ) -> Result<StampReport, RunFailure> {
        let run = self.new_run()?;
        // The server saves over the map it loaded when it quits, so it only ever loads a copy.
        let work = run.path().join("work.zip");
        std::fs::copy(base, &work)
            .map_err(|error| setup(format!("cannot copy {}: {error}", base.display())))?;
        let settings = run.path().join("server-settings.json");
        std::fs::write(&settings, SERVER_SETTINGS).map_err(setup)?;
        let args: Vec<OsString> = vec![
            "--start-server".into(),
            work.clone().into(),
            "--server-settings".into(),
            settings.into(),
            "--bind".into(),
            "127.0.0.1".into(),
            "--port".into(),
            "0".into(),
        ];
        let (mod_run, finished) =
            self.run_mod(base, &run, &Job::Stamp(job.clone()), &args, Ending::Quit)?;
        let report = mod_run.stamp.clone();
        let report = mod_run.conclude(report, finished)?;
        std::fs::copy(&work, output)
            .map_err(|error| setup(format!("cannot write {}: {error}", output.display())))?;
        Ok(report)
    }
}

/// A private server: loopback only (with `--bind 127.0.0.1`), unlisted, never pausing for lack
/// of players, never autosaving.
const SERVER_SETTINGS: &str = r#"{
  "name": "belt",
  "description": "B.E.L.T. stamping server",
  "visibility": { "public": false, "lan": false },
  "require_user_verification": false,
  "max_players": 0,
  "auto_pause": false,
  "auto_pause_when_players_connect": false,
  "autosave_interval": 0,
  "autosave_slots": 1,
  "non_blocking_saving": false,
  "only_admins_can_pause_the_game": true
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(code: Option<i32>) -> Finished {
        Finished {
            code,
            tail: vec!["last line".into()],
            interrupted: false,
        }
    }

    fn reason<T: std::fmt::Debug>(result: Result<T, RunFailure>) -> FailureReason {
        result.unwrap_err().reason
    }

    #[test]
    fn a_mod_error_reaches_the_cli_as_a_failed_save_with_its_message() {
        let mut run = ModRun::default();
        run.line(r#"BELT-SANITIZER {"type":"error","message":"blueprint has no entities"}"#);

        assert_eq!(
            reason(run.conclude(None::<()>, finished(Some(1)))),
            FailureReason::Mod("blueprint has no entities".into())
        );
    }

    #[test]
    fn an_exit_without_a_result_is_a_failure() {
        let run = ModRun::default();

        assert_eq!(
            reason(run.conclude(None::<()>, finished(Some(0)))),
            FailureReason::NoResult
        );
    }

    #[test]
    fn a_result_with_a_failing_exit_code_is_a_failure() {
        let run = ModRun::default();

        assert_eq!(
            reason(run.conclude(Some(()), finished(Some(255)))),
            FailureReason::Exited(Some(255))
        );
    }

    #[test]
    fn a_hello_with_another_interface_version_is_refused() {
        let mut run = ModRun::default();
        run.line(r#"BELT-SANITIZER {"type":"hello","interface":3}"#);

        assert!(matches!(
            reason(run.conclude(Some(()), finished(Some(0)))),
            FailureReason::Protocol(_)
        ));
    }

    #[test]
    fn an_interrupted_run_is_interrupted_even_with_a_result() {
        let run = ModRun::default();
        let finished = Finished {
            code: Some(0),
            tail: vec![],
            interrupted: true,
        };

        assert_eq!(
            reason(run.conclude(Some(()), finished)),
            FailureReason::Interrupted
        );
    }

    #[test]
    fn verbose_timings_are_requested_from_factorio_as_all() {
        let save = Save::new(PathBuf::from("/saves/a.zip"));

        let args = benchmark_args(
            &save,
            &BenchmarkRequest {
                ticks: 600,
                verbose: true,
            },
        );

        let args: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--benchmark",
                "/saves/a.zip",
                "--benchmark-ticks",
                "600",
                "--benchmark-runs",
                "1",
                "--benchmark-verbose",
                "all"
            ]
        );
    }

    mod sanitizer_download {
        use super::super::*;
        use crate::factorio::portal::fake::FakePortal;
        use sha1::{Digest, Sha1};

        /// A belt-sanitizer release zip of `version`, as the portal ships it.
        fn release_zip(version: &str) -> Vec<u8> {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(format!("belt-sanitizer_{version}/info.json"), options)
                .unwrap();
            let info = serde_json::json!({ "name": "belt-sanitizer", "version": version, "title": "t", "author": "a", "factorio_version": "2.1" });
            std::io::Write::write_all(&mut zip, info.to_string().as_bytes()).unwrap();
            zip.finish().unwrap().into_inner()
        }

        fn sha1_hex(bytes: &[u8]) -> String {
            Sha1::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        }

        /// A portal with releases for an older interface, this one, and a later Factorio.
        fn portal() -> (FakePortal, Vec<u8>) {
            let wanted = release_zip("2.0.0");
            let release = |version: &str, factorio: &str, sha1: &str| {
                serde_json::json!({
                    "version": version, "download_url": format!("/download/belt-sanitizer/{version}"),
                    "file_name": format!("belt-sanitizer_{version}.zip"), "sha1": sha1,
                    "info_json": { "factorio_version": factorio },
                })
            };
            let listing = serde_json::json!({ "name": "belt-sanitizer", "releases": [
                release("1.3.0", "2.1", "00"), release("2.0.0", "2.1", &sha1_hex(&wanted)), release("2.1.0", "2.2", "00"),
            ]});
            let routes = vec![
                (
                    "/api/mods/belt-sanitizer/full".to_string(),
                    listing.to_string().into_bytes(),
                ),
                ("/download/belt-sanitizer/2.0.0".to_string(), wanted.clone()),
            ];
            (FakePortal::serve(routes), wanted)
        }

        fn player_data(dir: &Path, json: &str) -> PathBuf {
            let path = dir.join("player-data.json");
            std::fs::write(&path, json).unwrap();
            path
        }

        const FACTORIO: Version = Version::new(2, 1, 25);

        #[test]
        fn the_sanitizer_release_for_this_interface_and_factorio_is_downloaded_from_the_portal() {
            let dir = tempfile::tempdir().unwrap();
            let (fake, wanted) = portal();
            let token = player_data(
                dir.path(),
                r#"{"service-username": "u", "service-token": "t"}"#,
            );

            let source = fetch_sanitizer(
                &Portal::at(&fake.base),
                &dir.path().join("downloads"),
                &token,
                FACTORIO,
            )
            .unwrap();

            let mods::SanitizerSource::Zip(path) = source else {
                panic!("{source:?}")
            };
            assert_eq!(std::fs::read(path).unwrap(), wanted);
        }

        #[test]
        fn without_a_token_the_sanitizer_download_fails_naming_where_it_looked() {
            let dir = tempfile::tempdir().unwrap();
            let (fake, _) = portal();
            let no_token = player_data(dir.path(), "{}");

            let error = fetch_sanitizer(
                &Portal::at(&fake.base),
                &dir.path().join("downloads"),
                &no_token,
                FACTORIO,
            )
            .unwrap_err();

            assert!(
                error.to_string().contains(&no_token.display().to_string()),
                "{error}"
            );
        }

        #[test]
        fn a_downloaded_release_whose_sha1_matches_is_reused_without_a_token() {
            let dir = tempfile::tempdir().unwrap();
            let (fake, wanted) = portal();
            let downloads = dir.path().join("downloads");
            std::fs::create_dir_all(&downloads).unwrap();
            std::fs::write(downloads.join("belt-sanitizer_2.0.0.zip"), wanted).unwrap();
            let no_token = player_data(dir.path(), "{}");

            let source = fetch_sanitizer(&Portal::at(&fake.base), &downloads, &no_token, FACTORIO);

            assert!(source.is_ok(), "{source:?}");
        }
    }
}
