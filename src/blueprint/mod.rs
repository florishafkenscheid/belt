//! `blueprint`: build blueprints into a base save, many copies each, and save the results.

pub mod string;

use std::path::{Path, PathBuf};

use crate::failure::{FailureReason, Failures, RunFailure};
use crate::protocol::{ItemWithQuality, StampJob, StampReport};
use crate::saves::file_stem;

/// Stamps one blueprint into a copy of `base` and writes the result to `output`. Implemented
/// by the Factorio adapter; `base` is never written.
pub trait Stamper {
    fn stamp(
        &mut self,
        base: &Path,
        job: &StampJob,
        output: &Path,
    ) -> Result<StampReport, RunFailure>;
}

/// The blueprint settings once every required one is known.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintRun {
    pub base_save: PathBuf,
    pub count: u32,
    pub buffer_ticks: u32,
    pub bot_count: u32,
    pub prefix: String,
    pub output: PathBuf,
    pub module_replacement: ItemWithQuality,
}

#[derive(Debug, thiserror::Error)]
pub enum BlueprintCommandError {
    #[error("cannot create output directory {path}: {source}")]
    OutputDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub failures: Failures,
    pub warnings: Vec<String>,
    pub interrupted: bool,
}

pub fn run(
    settings: &BlueprintRun,
    files: &[PathBuf],
    stamper: &mut dyn Stamper,
) -> Result<Outcome, BlueprintCommandError> {
    std::fs::create_dir_all(&settings.output).map_err(|source| {
        BlueprintCommandError::OutputDir {
            path: settings.output.clone(),
            source,
        }
    })?;
    let mut outcome = Outcome::default();
    for file in files {
        let stem = file_stem(file);
        let blueprints = match std::fs::read_to_string(file)
            .map_err(|error| error.to_string())
            .and_then(|text| string::blueprints(&stem, &text).map_err(|error| error.to_string()))
        {
            Ok(blueprints) => blueprints,
            Err(message) => {
                outcome.failures.add(
                    &stem,
                    RunFailure::new(FailureReason::Setup(format!(
                        "{}: {message}",
                        file.display()
                    ))),
                );
                continue;
            }
        };
        for blueprint in blueprints {
            let name = format!("{}{}", settings.prefix, blueprint.name);
            let output = settings.output.join(format!("{name}.zip"));
            let job = StampJob {
                name: name.clone(),
                blueprint: blueprint.string,
                count: settings.count,
                buffer_ticks: settings.buffer_ticks,
                bot_count: settings.bot_count,
                module_replacement: settings.module_replacement.clone(),
            };
            eprintln!("stamping {name} ({} copies)", settings.count);
            let report = match stamper.stamp(&settings.base_save, &job, &output) {
                Ok(report) => report,
                Err(failure) if failure.reason == FailureReason::Interrupted => {
                    outcome.interrupted = true;
                    return Ok(outcome);
                }
                Err(failure) => {
                    outcome.failures.add(&name, failure);
                    continue;
                }
            };
            outcome.warnings.extend(warnings(&name, &report));
            if !report.unplaced.is_empty() {
                // The blueprint fails as a whole, so its save must not look like a result.
                if let Err(error) = std::fs::remove_file(&output) {
                    outcome.warnings.push(format!(
                        "{name}: could not remove the incomplete save {}: {error}",
                        output.display()
                    ));
                }
                outcome.failures.add(
                    &name,
                    RunFailure::new(FailureReason::Unplaced(report.unplaced)),
                );
            }
        }
    }
    for warning in &outcome.warnings {
        eprintln!("WARNING {warning}");
    }
    Ok(outcome)
}

fn warnings(name: &str, report: &StampReport) -> Vec<String> {
    let mut warnings = Vec::new();
    if report.unmarked_drills > 0 {
        warnings.push(format!(
            "{name}: {} mining drills have no marker and were built without ore",
            report.unmarked_drills
        ));
    }
    for resource in &report.missing_resources {
        warnings.push(format!(
            "{name}: resource {resource} doesn't exist in this save; its drills got no ore"
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Unplaced;
    use serde_json::json;

    #[derive(Default)]
    struct FakeStamper {
        outputs: Vec<PathBuf>,
        report: Option<StampReport>,
    }

    impl Stamper for FakeStamper {
        fn stamp(
            &mut self,
            _base: &Path,
            _job: &StampJob,
            output: &Path,
        ) -> Result<StampReport, RunFailure> {
            std::fs::write(output, "save").unwrap();
            self.outputs.push(output.to_path_buf());
            Ok(self.report.clone().unwrap_or_else(|| report(0, vec![])))
        }
    }

    fn report(unmarked_drills: u64, unplaced: Vec<Unplaced>) -> StampReport {
        StampReport {
            expected: 1,
            placed: 1,
            stamped_tick: 1,
            unmarked_drills,
            missing_resources: vec![],
            unplaced,
        }
    }

    fn settings(output: &Path) -> BlueprintRun {
        BlueprintRun {
            base_save: PathBuf::from("/saves/base.zip"),
            count: 2,
            buffer_ticks: 60,
            bot_count: 0,
            prefix: "bp-".into(),
            output: output.to_path_buf(),
            module_replacement: ItemWithQuality {
                name: "speed-module-3".into(),
                quality: "legendary".into(),
            },
        }
    }

    fn file(dir: &Path, name: &str, value: serde_json::Value) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, string::encode(&value)).unwrap();
        path
    }

    fn file_names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn each_blueprint_file_becomes_one_save_named_after_the_prefix_and_file_stem() {
        let dir = tempfile::tempdir().unwrap();
        let input = file(dir.path(), "smelting.txt", json!({ "blueprint": {} }));
        let mut stamper = FakeStamper::default();

        run(&settings(&dir.path().join("out")), &[input], &mut stamper).unwrap();

        assert_eq!(file_names(&stamper.outputs), vec!["bp-smelting.zip"]);
    }

    #[test]
    fn a_blueprint_book_becomes_one_save_per_blueprint_numbered_from_one() {
        let dir = tempfile::tempdir().unwrap();
        let entries = json!([{ "index": 0, "blueprint": {} }, { "index": 1, "blueprint": {} }]);
        let input = file(
            dir.path(),
            "mall.txt",
            json!({ "blueprint_book": { "blueprints": entries } }),
        );
        let mut stamper = FakeStamper::default();

        run(&settings(&dir.path().join("out")), &[input], &mut stamper).unwrap();

        assert_eq!(
            file_names(&stamper.outputs),
            vec!["bp-mall_1.zip", "bp-mall_2.zip"]
        );
    }

    #[test]
    fn unmarked_drills_are_reported_per_blueprint_as_a_warning() {
        let dir = tempfile::tempdir().unwrap();
        let input = file(dir.path(), "mine.txt", json!({ "blueprint": {} }));
        let mut stamper = FakeStamper {
            report: Some(report(42, vec![])),
            ..Default::default()
        };

        let outcome = run(&settings(&dir.path().join("out")), &[input], &mut stamper).unwrap();

        assert_eq!(
            outcome.warnings,
            vec!["bp-mine: 42 mining drills have no marker and were built without ore"]
        );
    }

    #[test]
    fn an_unplaced_entity_fails_the_blueprint_with_its_reason() {
        let dir = tempfile::tempdir().unwrap();
        let input = file(dir.path(), "mine.txt", json!({ "blueprint": {} }));
        let unplaced = Unplaced {
            name: "beacon".into(),
            x: 1.5,
            y: 2.5,
            reason: "blocked by rock-big".into(),
        };
        let mut stamper = FakeStamper {
            report: Some(report(0, vec![unplaced])),
            ..Default::default()
        };

        let outcome = run(&settings(&dir.path().join("out")), &[input], &mut stamper).unwrap();

        let saved = dir.path().join("out/bp-mine.zip").exists();
        assert_eq!((outcome.failures.is_empty(), saved), (false, false));
    }
}
