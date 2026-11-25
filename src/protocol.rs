//! The interface between this CLI and the `belt-sanitizer` mod, which lives in its own repository.
//!
//! The CLI hands the mod a job as a generated `job.lua` inside the mod. The mod answers with
//! lines on stdout that start with [`PREFIX`], one JSON message per line. Both sides check
//! [`INTERFACE`]: the mod's `info.json` major version must equal it, the mod refuses a job with
//! another value, and the CLI refuses a `hello` with another value.

use serde::{Deserialize, Serialize};

/// The interface version. Bump it, and the mod's major version, on any change to the job or
/// message shapes that either side can't ignore.
pub const INTERFACE: u32 = 2;

/// The mod's internal name, as Factorio knows it.
pub const MOD_NAME: &str = "belt-sanitizer";

/// Every message line starts with this. Lines without it are Factorio's own output.
pub const PREFIX: &str = "BELT-SANITIZER ";

/// The work for one Factorio run.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Job {
    Sanitize(SanitizeJob),
    Stamp(StampJob),
}

/// Run the save for `ticks` ticks, then report what would skew a benchmark and the production
/// and consumption of the tracked items and fluids over those ticks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SanitizeJob {
    pub ticks: u32,
    pub items: Vec<String>,
    pub fluids: Vec<String>,
}

/// Build `count` copies of one blueprint near spawn, simulate `buffer_ticks`, then report.
/// The CLI saves the map once the result arrives.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StampJob {
    /// Shown in the mod's warnings, so they name the blueprint.
    pub name: String,
    /// A blueprint string holding one blueprint (never a book).
    pub blueprint: String,
    pub count: u32,
    pub buffer_ticks: u32,
    pub bot_count: u32,
    /// Replaces the module requests of every drill that had a marker.
    pub module_replacement: ItemWithQuality,
}

/// An item prototype name with a quality prototype name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemWithQuality {
    pub name: String,
    pub quality: String,
}

#[derive(Serialize)]
struct JobFile<'a> {
    interface: u32,
    #[serde(flatten)]
    job: &'a Job,
}

/// The contents of `job.lua`: a Lua chunk returning the job as a JSON string, wrapped in a long
/// bracket whose level doesn't occur in the JSON, so no payload can end the string early.
pub fn job_lua(job: &Job) -> String {
    let json = serde_json::to_string(&JobFile {
        interface: INTERFACE,
        job,
    })
    .expect("a job always serializes");
    let level = (0..)
        .map(|level| "=".repeat(level))
        .find(|equals| !json.contains(&format!("]{equals}]")))
        .expect("some level is free");
    format!("return [{level}[{json}]{level}]\n")
}

/// One message from the mod.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Sent first, from the main chunk. `interface` must equal [`INTERFACE`].
    Hello { interface: u32 },
    /// Something the user should see. The run continues.
    Warning { text: String },
    /// The mod gave up. The run ends after this line.
    Error { message: String },
    /// The final answer to a [`Job::Sanitize`].
    SanitizeResult { report: SanitizeReport },
    /// The final answer to a [`Job::Stamp`]. The map is ready to save.
    StampResult { report: StampReport },
}

/// What a sanitize run found. Lists the mod has nothing for are omitted on the wire.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SanitizeReport {
    pub pollution_enabled: bool,
    pub expansion_enabled: bool,
    #[serde(default)]
    pub surfaces: Vec<SurfaceReport>,
    /// One entry per tracked item and quality, summed over all surfaces and forces.
    #[serde(default)]
    pub items: Vec<ItemFlow>,
    /// One entry per tracked fluid, summed over all surfaces and forces.
    #[serde(default)]
    pub fluids: Vec<FluidFlow>,
}

/// Per-surface findings at the end of a sanitize run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceReport {
    pub name: String,
    /// Total pollution on the surface.
    pub pollution: f64,
    pub enemy_units: u64,
    pub enemy_spawners: u64,
    pub enemy_worms: u64,
    /// Active rocket silos, cars and tanks, by prototype name.
    #[serde(default)]
    pub vehicles: Vec<Count>,
    /// Infinity and similar test entities, by prototype type.
    #[serde(default)]
    pub infinity: Vec<Count>,
}

/// How many of one kind.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Count {
    pub name: String,
    pub count: u64,
}

/// Production and consumption of one item at one quality over the run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ItemFlow {
    pub name: String,
    pub quality: String,
    pub produced: f64,
    pub consumed: f64,
}

/// Production and consumption of one fluid over the run.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FluidFlow {
    pub name: String,
    pub produced: f64,
    pub consumed: f64,
}

/// What a stamp run built.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StampReport {
    /// Entities of all copies, as the blueprint asks for them.
    pub expected: u64,
    /// Real (not ghost) entities that ended up on the map.
    pub placed: u64,
    /// The map tick at which building finished; the save follows `buffer_ticks` later.
    pub stamped_tick: u64,
    /// Drills in the blueprint (one copy) without a marker, built without ore.
    pub unmarked_drills: u64,
    /// Marker resources that don't exist in this save, so their drills got no ore.
    #[serde(default)]
    pub missing_resources: Vec<String>,
    /// Entities that couldn't be placed. Any entry fails the blueprint.
    #[serde(default)]
    pub unplaced: Vec<Unplaced>,
}

/// One blueprint entity that isn't on the map, and why.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Unplaced {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub reason: String,
}

/// One line of Factorio's stdout, classified.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// Factorio's own output.
    Factorio,
    Message(Message),
}

/// A prefixed line that isn't a valid message.
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("unreadable message from {MOD_NAME}: {0}")]
pub struct ProtocolError(String);

/// Classifies a line of stdout. A prefixed line must parse, so a mod that speaks another
/// dialect fails loudly instead of being ignored.
pub fn parse_line(line: &str) -> Result<Line, ProtocolError> {
    let Some(json) = line.strip_prefix(PREFIX) else {
        return Ok(Line::Factorio);
    };
    serde_json::from_str(json)
        .map(Line::Message)
        .map_err(|error| ProtocolError(format!("{error}: {json}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitize_job() -> Job {
        Job::Sanitize(SanitizeJob {
            ticks: 60,
            items: vec!["iron-plate".into()],
            fluids: vec![],
        })
    }

    #[test]
    fn the_job_file_returns_the_job_json_with_the_interface_version() {
        let lua = job_lua(&sanitize_job());

        assert_eq!(
            lua,
            "return [[{\"interface\":2,\"kind\":\"sanitize\",\"ticks\":60,\"items\":[\"iron-plate\"],\"fluids\":[]}]]\n"
        );
    }

    #[test]
    fn the_job_file_bracket_level_avoids_closing_brackets_in_the_payload() {
        let job = Job::Sanitize(SanitizeJob {
            ticks: 1,
            items: vec!["a]]b]=]c".into()],
            fluids: vec![],
        });

        let lua = job_lua(&job);

        assert!(lua.starts_with("return [==[{"), "{lua}");
    }

    #[test]
    fn a_line_without_the_prefix_is_factorio_output() {
        assert_eq!(
            parse_line("   0.512 Factorio initialised"),
            Ok(Line::Factorio)
        );
    }

    #[test]
    fn a_prefixed_line_is_a_message() {
        let line = parse_line(r#"BELT-SANITIZER {"type":"hello","interface":1}"#);

        assert_eq!(line, Ok(Line::Message(Message::Hello { interface: 1 })));
    }

    #[test]
    fn a_prefixed_line_that_does_not_parse_is_a_protocol_error() {
        assert!(parse_line(r#"BELT-SANITIZER {"type":"shout"}"#).is_err());
    }

    #[test]
    fn omitted_lists_in_a_report_are_empty() {
        let line = parse_line(
            r#"BELT-SANITIZER {"type":"sanitize_result","report":{"pollution_enabled":false,"expansion_enabled":false}}"#,
        );

        let Ok(Line::Message(Message::SanitizeResult { report })) = line else {
            panic!("{line:?}")
        };
        assert!(report.surfaces.is_empty() && report.items.is_empty() && report.fluids.is_empty());
    }
}
