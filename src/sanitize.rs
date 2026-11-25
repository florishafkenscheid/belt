//! `sanitize`: run saves for a while and report what would skew a benchmark.

use std::collections::BTreeMap;

use crate::failure::{FailureReason, Failures, RunFailure};
use crate::protocol::{SanitizeJob, SanitizeReport};
use crate::saves::Save;
use crate::terminal::Progress;

/// Runs one save with the job and returns the mod's report. Implemented by the Factorio
/// adapter; the save is never written.
pub trait Inspector {
    fn sanitize(&mut self, save: &Save, job: &SanitizeJob) -> Result<SanitizeReport, RunFailure>;
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub failures: Failures,
    pub interrupted: bool,
}

pub fn run(job: &SanitizeJob, saves: &[Save], inspector: &mut dyn Inspector) -> Outcome {
    let mut outcome = Outcome::default();
    let progress = Progress::new(saves.len());
    for (position, save) in saves.iter().enumerate() {
        progress.start(position, &save.name);
        match inspector.sanitize(save, job) {
            Ok(report) => log(&save.name, &render(&report)),
            Err(failure) if failure.reason == FailureReason::Interrupted => {
                outcome.interrupted = true;
                break;
            }
            Err(failure) => outcome.failures.add(&save.name, failure),
        }
    }
    if outcome.interrupted {
        progress.stop("Sanitization interrupted");
    } else {
        progress.finish("Sanitization complete!");
    }
    outcome
}

/// One line of a save's terminal report.
#[derive(Debug, PartialEq)]
pub enum ReportLine {
    /// Something that would skew a benchmark.
    Problem(String),
    /// A tracked item's or fluid's production and consumption.
    Flow(String),
}

/// Logs a save's report under its name: problems as warnings, flows as information.
fn log(name: &str, lines: &[ReportLine]) {
    tracing::info!("== {name} ==");
    for line in lines {
        match line {
            ReportLine::Problem(text) => tracing::warn!("{text}"),
            ReportLine::Flow(text) => tracing::info!("{text}"),
        }
    }
}

/// The terminal report of one save. Flows leave out zero counts.
pub fn render(report: &SanitizeReport) -> Vec<ReportLine> {
    let mut lines = Vec::new();
    let mut warn = |text: String| lines.push(ReportLine::Problem(text));
    if report.pollution_enabled {
        warn("pollution is enabled".into());
    }
    for surface in report
        .surfaces
        .iter()
        .filter(|surface| surface.pollution > 0.0)
    {
        warn(format!(
            "{}: {} pollution on the map",
            surface.name, surface.pollution
        ));
    }
    if report.expansion_enabled {
        warn("enemy expansion is enabled".into());
    }
    for surface in &report.surfaces {
        let enemies = [
            ("units", surface.enemy_units),
            ("spawners", surface.enemy_spawners),
            ("worms", surface.enemy_worms),
        ];
        let present: Vec<String> = enemies
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        if !present.is_empty() {
            warn(format!("{}: enemies: {}", surface.name, present.join(", ")));
        }
        for vehicle in &surface.vehicles {
            warn(format!(
                "{}: {} active {}",
                surface.name, vehicle.count, vehicle.name
            ));
        }
        for infinity in &surface.infinity {
            warn(format!(
                "{}: {} {} (infinity/test entity)",
                surface.name, infinity.count, infinity.name
            ));
        }
    }
    let mut all_qualities: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for item in &report.items {
        if let Some(text) = flow(item.produced, item.consumed) {
            lines.push(ReportLine::Flow(format!(
                "{} ({}): {text}",
                item.name, item.quality
            )));
        }
        let total = all_qualities.entry(&item.name).or_default();
        total.0 += item.produced;
        total.1 += item.consumed;
    }
    for (item, (produced, consumed)) in all_qualities {
        if let Some(text) = flow(produced, consumed) {
            lines.push(ReportLine::Flow(format!("{item} (all qualities): {text}")));
        }
    }
    for fluid in &report.fluids {
        if let Some(text) = flow(fluid.produced, fluid.consumed) {
            lines.push(ReportLine::Flow(format!("{}: {text}", fluid.name)));
        }
    }
    lines
}

/// "produced X, consumed Y" without its zero parts, or nothing when both are zero.
fn flow(produced: f64, consumed: f64) -> Option<String> {
    let parts: Vec<String> = [("produced", produced), ("consumed", consumed)]
        .into_iter()
        .filter(|(_, amount)| *amount != 0.0)
        .map(|(verb, amount)| format!("{verb} {amount}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{FluidFlow, ItemFlow};

    fn quiet() -> SanitizeReport {
        SanitizeReport {
            pollution_enabled: false,
            expansion_enabled: false,
            surfaces: vec![],
            items: vec![],
            fluids: vec![],
        }
    }

    fn item(quality: &str, produced: f64, consumed: f64) -> ItemFlow {
        ItemFlow {
            name: "iron-plate".into(),
            quality: quality.into(),
            produced,
            consumed,
        }
    }

    #[test]
    fn reports_production_and_consumption_per_item_quality_and_fluid() {
        let report = SanitizeReport {
            items: vec![item("normal", 10.0, 4.0), item("rare", 2.0, 1.0)],
            fluids: vec![FluidFlow {
                name: "water".into(),
                produced: 1500.5,
                consumed: 1200.0,
            }],
            ..quiet()
        };

        assert_eq!(
            render(&report),
            [
                "iron-plate (normal): produced 10, consumed 4",
                "iron-plate (rare): produced 2, consumed 1",
                "iron-plate (all qualities): produced 12, consumed 5",
                "water: produced 1500.5, consumed 1200",
            ]
            .map(|text| ReportLine::Flow(text.into()))
        );
    }

    #[test]
    fn leaves_out_zero_counts() {
        let report = SanitizeReport {
            items: vec![item("normal", 10.0, 0.0), item("epic", 0.0, 0.0)],
            ..quiet()
        };

        assert_eq!(
            render(&report),
            [
                "iron-plate (normal): produced 10",
                "iron-plate (all qualities): produced 10",
            ]
            .map(|text| ReportLine::Flow(text.into()))
        );
    }
}
