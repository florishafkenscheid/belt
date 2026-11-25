mod benchmark;
mod blueprint;
mod cli;
mod config;
mod cpu;
mod factorio;
mod failure;
mod protocol;
mod sanitize;
mod saves;
mod version;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use directories::BaseDirs;

use cli::{Cli, CommandKind};
use config::{ConfigFile, Settings, required, split_list};
use factorio::discovery::{self, Places};
use factorio::launch::Command;
use factorio::portal::Portal;
use factorio::workspace::Workspace;
use factorio::{Factorio, FactorioOptions, ModSource, SanitizerMod};
use failure::Failures;
use protocol::{ItemWithQuality, SanitizeJob};
use saves::{Kind, Save};

/// Exit codes: every save succeeded; a save failed or Ctrl-C stopped the command; the command
/// itself couldn't run.
const SUCCESS: u8 = 0;
const SAVES_FAILED: u8 = 1;
const COMMAND_FAILED: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(COMMAND_FAILED)
        }
    }
}

type BoxError = Box<dyn std::error::Error>;

/// `<OS config dir>/belt`, as in 4.x (`~/.config/belt`, `%APPDATA%\belt`, …).
fn config_dir(base_dirs: &BaseDirs) -> PathBuf {
    base_dirs.config_dir().join("belt")
}

fn run(cli: &Cli) -> Result<u8, BoxError> {
    let base_dirs = BaseDirs::new().ok_or("cannot determine the home directory")?;
    if cli.version {
        println!("{} v{}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
        return Ok(SUCCESS);
    }
    if cli.init_config {
        return init_config(&config_dir(&base_dirs).join("config.toml"));
    }
    let Some(command) = &cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(SUCCESS);
    };
    let file = match &cli.config {
        Some(path) => ConfigFile::Explicit(path.clone()),
        None => ConfigFile::Default(config_dir(&base_dirs).join("config.toml")),
    };
    let settings = config::resolve(cli.flags(), |name| std::env::var(name).ok(), &file)?;
    let root = PathBuf::from(if std::env::consts::OS == "windows" {
        r"C:\"
    } else {
        "/"
    });
    let places = Places {
        os: std::env::consts::OS,
        home: base_dirs.home_dir(),
        root: &root,
        roaming_app_data: base_dirs.config_dir(),
    };
    let context = Context {
        settings: &settings,
        places: &places,
        base_dirs: &base_dirs,
    };
    match command {
        CommandKind::Benchmark(_) => {
            let benchmark = &settings.benchmark;
            let saves = select_saves(
                benchmark.saves_dir.clone(),
                &benchmark.pattern,
                "benchmark",
                &file,
            )?;
            let mods = Mods {
                mods_dir: &benchmark.mods_dir,
                data_dir: &benchmark.data_dir,
            };
            let mut factorio =
                context.open_factorio(mods, SanitizerMod::NotNeeded, Command::Benchmark)?;
            let monitor: Box<dyn benchmark::CpuMonitor> = if benchmark.record_cpu {
                Box::new(cpu::SystemCpuMonitor)
            } else {
                Box::new(benchmark::NoCpuMonitor)
            };
            let date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let outcome = benchmark::run(
                benchmark,
                &saves,
                &mut factorio,
                monitor.as_ref(),
                &date,
                &mut fastrand::Rng::new(),
            )?;
            Ok(finish(&outcome.failures, outcome.interrupted))
        }
        CommandKind::Blueprint(_) => {
            let blueprint = &settings.blueprint;
            let dir = required(
                blueprint.blueprints_dir.clone(),
                "blueprint",
                "blueprints_dir",
                "the BLUEPRINTS_DIR argument",
                &file,
            )?;
            let files = saves::select(&dir, &blueprint.pattern, Kind::Blueprints)?;
            let run = blueprint::BlueprintRun {
                base_save: required(
                    blueprint.base_save_path.clone(),
                    "blueprint",
                    "base_save_path",
                    "the BASE_SAVE_PATH argument",
                    &file,
                )?,
                count: required(blueprint.count, "blueprint", "count", "--count", &file)?,
                buffer_ticks: required(
                    blueprint.buffer_ticks,
                    "blueprint",
                    "buffer_ticks",
                    "--buffer-ticks",
                    &file,
                )?,
                bot_count: blueprint.bot_count,
                prefix: blueprint.prefix.clone(),
                output: blueprint.output.clone(),
                module_replacement: ItemWithQuality {
                    name: blueprint.mining_module_replacement.clone(),
                    quality: blueprint.mining_module_replacement_quality.clone(),
                },
            };
            let mods = Mods {
                mods_dir: &blueprint.mods_dir,
                data_dir: &blueprint.data_dir,
            };
            let mut factorio =
                context.open_factorio(mods, context.sanitizer_mod(), Command::Blueprint)?;
            let outcome = blueprint::run(&run, &files, &mut factorio)?;
            Ok(finish(&outcome.failures, outcome.interrupted))
        }
        CommandKind::Sanitize(_) => {
            let sanitize = &settings.sanitize;
            let saves = select_saves(
                sanitize.saves_dir.clone(),
                &sanitize.pattern,
                "sanitize",
                &file,
            )?;
            let mods = Mods {
                mods_dir: &sanitize.mods_dir,
                data_dir: &sanitize.data_dir,
            };
            let mut factorio =
                context.open_factorio(mods, context.sanitizer_mod(), Command::Sanitize)?;
            let job = SanitizeJob {
                ticks: sanitize.ticks,
                items: split_list(&sanitize.items),
                fluids: split_list(&sanitize.fluids),
            };
            let outcome = sanitize::run(&job, &saves, &mut factorio);
            Ok(finish(&outcome.failures, outcome.interrupted))
        }
    }
}

/// Writes the commented example config, as 4.x's `--init-config` does: an existing file is kept.
fn init_config(path: &Path) -> Result<u8, BoxError> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, config::default_file())?;
    }
    println!("Initialized config directory at: {}", path.display());
    Ok(SUCCESS)
}

fn select_saves(
    dir: Option<PathBuf>,
    pattern: &str,
    section: &'static str,
    file: &ConfigFile,
) -> Result<Vec<Save>, BoxError> {
    let dir = required(dir, section, "saves_dir", "the SAVES_DIR argument", file)?;
    Ok(saves::select(&dir, pattern, Kind::Saves)?
        .into_iter()
        .map(Save::new)
        .collect())
}

/// A command's mod settings: an explicit mods directory, and Factorio's user data directory.
struct Mods<'a> {
    mods_dir: &'a Option<PathBuf>,
    data_dir: &'a Option<PathBuf>,
}

/// What every command needs to start Factorio.
struct Context<'a> {
    settings: &'a Settings,
    places: &'a Places<'a>,
    base_dirs: &'a BaseDirs,
}

impl Context<'_> {
    /// The belt-sanitizer mod: the given folder, or else the mod portal's release.
    fn sanitizer_mod(&self) -> SanitizerMod {
        match &self.settings.global.sanitizer_mod {
            Some(folder) => SanitizerMod::Folder(folder.clone()),
            None => SanitizerMod::Portal,
        }
    }

    fn open_factorio(
        &self,
        mods: Mods,
        sanitizer: SanitizerMod,
        command: Command,
    ) -> Result<Factorio, BoxError> {
        let global = &self.settings.global;
        let executable = discovery::find_factorio(global.factorio_path.as_deref(), self.places)?;
        let workspace_dir = match &global.workspace_dir {
            Some(dir) => dir.clone(),
            None => self.base_dirs.data_dir().join("belt"),
        };
        let data_dir = match mods.data_dir {
            Some(dir) => dir.clone(),
            None => discovery::default_data_dir(self.places)?,
        };
        let source = match mods.mods_dir {
            Some(dir) => ModSource::Directory(dir.clone()),
            None => ModSource::Recorded {
                user_mods: data_dir.join("mods"),
            },
        };
        let options = FactorioOptions {
            executable,
            workspace: Workspace::open(&workspace_dir)?,
            mods: source,
            player_data: data_dir.join("player-data.json"),
            sanitizer,
            portal: Portal::official(),
            command,
            verbose: global.verbose,
        };
        Ok(Factorio::new(options)?)
    }
}

/// Prints the failure summary and picks the exit code.
fn finish(failures: &Failures, interrupted: bool) -> u8 {
    if interrupted {
        eprintln!("interrupted; completed runs were written");
    }
    if !failures.is_empty() {
        eprint!("{}", failures.summary());
    }
    if failures.is_empty() && !interrupted {
        SUCCESS
    } else {
        SAVES_FAILED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_config_keeps_an_existing_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("belt/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[benchmark]\nticks = 1\n").unwrap();

        init_config(&path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[benchmark]\nticks = 1\n"
        );
    }
}
