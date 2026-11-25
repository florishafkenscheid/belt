# BELT: Benchmark for Engine Limits & Throughput

![Crates.io Version](<https://img.shields.io/crates/v/belt?color=rgb(215%2C127%2C0)>)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/ci.yml?label=master)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/release.yml?label=release)

BELT is a benchmarking and testing suite for Factorio: it measures how fast saves run, turns blueprints into benchmark saves, and checks saves for anything that would skew a benchmark.

## Features

- [x] **Benchmarking** - Benchmark a single save or a whole directory
- [x] **Blueprint testing** - Stamp blueprints into a base save, many copies each, ready to benchmark
- [x] **Sanitizer** - Report what would skew a benchmark, and track item/fluid production, using the [belt-sanitizer mod](https://mods.factorio.com/mod/belt-sanitizer)
- [x] **Leaves Factorio alone** - Your Factorio settings, saves, mod list and mods directory are never changed, not even after a crash
- [x] **Cross-platform** - Works on Windows, macOS, and Linux
- [x] **Multiple output formats** - CSV and Markdown reports, with custom Handlebars templates
- [x] **Pattern matching** - Filter save files by name patterns
- [x] **Verbose metrics support** - Per-tick CSV exports for detailed Factorio metrics
- [x] **Chart-friendly exports** - Clean benchmark artifacts that can be visualized by external tooling such as `belt-charts`

## Quick Start

```bash
# Install BELT
cargo install belt

# Run benchmarks on all saves in a directory
belt benchmark ./saves --ticks 6000 --runs 5

# Filter saves by pattern and customize output directory
belt benchmark ./saves --pattern "inserter*" --output ./benchmark-results
```

## Installation

### Prerequisites

1. Factorio 2.1 installed. BELT searches common installation paths; if none are found, run with an explicit `--factorio-path`. `sanitize` needs Factorio 2.1.24 or newer.
2. Some save files to benchmark.
3. For `blueprint` and `sanitize`: a mod portal token, which Factorio stores in `player-data.json` once you've logged in to the mod portal in-game. BELT uses it to download the belt-sanitizer mod (and any save mod you don't have installed).
4. Rust if installing using cargo or building from source.

### Windows

Follow [this](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup#install-visual-studio-recommended-or-the-microsoft-c-build-tools) Visual C++ guide

### From Crates.io

```bash
cargo install belt
```

### From GitHub Releases

1. Download the latest binary for your platform from [Releases](https://github.com/florishafkenscheid/belt/releases)
2. Extract and place in your PATH

### From Source

```bash
git clone https://github.com/florishafkenscheid/belt.git
cd belt
cargo install --path .
```

## Usage

### Basic Commands

```bash
# Basic benchmark with default settings
belt benchmark /path/to/saves

# Customize benchmark parameters
belt benchmark /path/to/saves --ticks 12000 --runs 10

# Filter saves and specify output location
belt benchmark /path/to/saves --pattern "benchmark*" --output /path/to/output/dir
```

### How BELT runs Factorio

Every Factorio launch gets its own temporary configuration, so Factorio's log, settings and saves are written to a temporary run folder instead of your Factorio data directory. Mods are prepared in BELT's own mod folder (under `--workspace-dir`), never in your mods directory.

By default each save runs with the mods it was saved with, at the versions it recorded, using Factorio's own mod syncing. Mods you have installed are copied into BELT's mod folder; mods you don't have are downloaded from the mod portal. If a save recorded a mod version made for an older Factorio (a 2.0 mod on 2.1, say), BELT uses the newest version that works with your Factorio instead and prints a warning for every swap:

```text
 WARN belt::factorio: red_science: EditorExtensions 2.4.2 → 2.6.1 (2.4.2 is for Factorio 2.0)
```

A mod with no version that works with your Factorio fails that save, naming the mod. Pass `--mods-dir` to use one directory of mods for every save instead: each enabled mod then runs at the version that directory's `mod-list.json` pins, or else the newest one in it.

If BELT is killed while Factorio runs, the next BELT start stops whatever Factorio it left behind and removes its temporary folders, and says so.

### Command Reference

#### `belt benchmark`

Run benchmarks on one or more save files.

**Arguments:**

- `[SAVES_DIR]` - A directory of saves, or a single save file, to be benchmarked. Required unless `benchmark.saves_dir` is set in config.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--ticks <TICKS>` | How many ticks per run to run the benchmark for | `6000` |
| `--runs <RUNS>` | How many runs per save file | `5` |
| `--pattern <PATTERN>` | A glob matched against save names without `.zip` (`CHOAM`, `*car`) in `<SAVES_DIR>` | `*` |
| `--output <OUTPUT_DIR>` | A directory to output the .csv and .md files to | `.` |
| `--template-path <TEMPLATE>` | A Handlebars template for the report, instead of the built-in `results.md` | built-in |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for every save | each save's recorded mods |
| `--data-dir <DATA_DIR>` | Factorio's user data directory (your mods and `player-data.json`) | OS default |
| `--run-order <RUN_ORDER>` | In which order to run the benchmarks. Available: `sequential`, `random`, `grouped` | `sequential` |
| `--verbose-metrics <VERBOSE_METRICS>` | Exports per-tick verbose metric CSVs for the selected metrics, or `all` | `none` |
| `--strip-prefix <PREFIX>` | Strip a given prefix off of the save names | `none` |
| `--record-cpu` | Record CPU frequency samples during benchmark runs (turn off with `record_cpu = false` in config) | `true` |
| `--append` | Append benchmark rows to existing output CSV files. Existing CSV headers must match the current output format and selected verbose metrics. | `false` |
| `--compress` | Write verbose metric CSVs zstd-compressed as `<save>_verbose_metrics.csv.zst`. `results.csv` and `cpu_freq.csv` stay plain. | `false` |

#### `belt blueprint`

Stamp blueprints into a base save.

**Arguments:**

- `<BLUEPRINTS_DIR>` - A directory of blueprint files, or a single blueprint file.
- `<BASE_SAVE_PATH>` - Base save file to stamp blueprints into. It is never modified.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--count <COUNT>` | Number of times to stamp each blueprint. | **required** |
| `--buffer-ticks <BUFFER_TICKS>` | Ticks the game runs after stamping, before the save is written. | **required** |
| `--mining-module-replacement <MINING_MODULE_REPLACEMENT>` | Module to insert into mining drills after ore marker modules have been interpreted. | `speed-module-3` |
| `--mining-module-replacement-quality <MINING_MODULE_REPLACEMENT_QUALITY>` | Quality of replacement modules inserted into mining drills. | `legendary` |
| `--bot-count <BOT_COUNT>` | Logistic robots added to each copy's roboport network. | `0` |
| `--prefix <PREFIX>` | Prefix to add to generated save names. | `none` |
| `--pattern <PATTERN>` | Pattern to match against when searching for blueprint files. | `*` |
| `--output <OUTPUT_DIR>` | Directory to output generated saves. | `.` |
| `--mods-dir <MODS_DIR>` | Directory containing mods to use. | the base save's recorded mods |
| `--data-dir <DATA_DIR>` | Factorio's user data directory (your mods and `player-data.json`). | OS default |

Each blueprint file becomes one save, named `<prefix><file name>`; a blueprint book becomes one save per blueprint, named `<prefix><file name>_<n>`. The copies are built fully (not as ghosts) in a compact rectangle near spawn, on cleared ground, raising the script build and revive events so other mods see them. The player character is moved out of the way first. Anything that can't be placed is listed with its reason, and that blueprint's save isn't written.

Mining drills are built on ore patches chosen by markers in the blueprint:

| Marker                         | Resource                                                                                                                     |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `stone-path` tile              | `stone`                                                                                                                      |
| `concrete` tile                | `iron-ore`                                                                                                                   |
| `hazard-concrete` tile         | `coal`                                                                                                                       |
| `refined-concrete` tile        | `copper-ore`                                                                                                                 |
| `refined-hazard-concrete` tile | `scrap`                                                                                                                      |
| `efficiency-module` request    | `stone`, `iron-ore`, `copper-ore`, `coal`, or `uranium-ore` for `normal`, `uncommon`, `rare`, `epic`, or `legendary` quality |
| `efficiency-module-2` request  | `calcite`, `tungsten-ore`, or `scrap` for `normal`, `uncommon`, or `rare` quality                                            |

Tile markers under or next to a drill take precedence over module markers. After the resource is selected, mining-drill module requests are replaced with `--mining-module-replacement` and `--mining-module-replacement-quality`, so marker modules never end up in the save. A drill without a marker is built without ore, and listed as a warning. A marker whose resource doesn't exist in the save (Space Age resources without Space Age) is a warning too.

#### `belt sanitize`

Run saves for a while and report anything that would skew a benchmark, plus the production and consumption of chosen items and fluids. Needs Factorio 2.1.24 or newer.

**Arguments:**

- `[SAVES_DIR]` - A directory of saves, or a single save file, to be sanitized. Required unless `sanitize.saves_dir` is set in config.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--pattern <PATTERN>` | A glob matched against save names without `.zip` (`CHOAM`, `*car`) in `<SAVES_DIR>` | `*` |
| `--ticks <TICKS>` | How many ticks to run the sanitization for | `3600` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for every save | each save's recorded mods |
| `--data-dir <DATA_DIR>` | If B.E.L.T. can't find your user data directory, pass it explicitly here. | OS default |
| `--items <ITEMS>` | A comma separated list of items to track. | `none` |
| `--fluids <FLUIDS>` | A comma separated list of fluids to track. | `none` |

The report warns about pollution (enabled or present), enemy expansion, enemies per surface, active rocket silos, cars and tanks, and infinity/test entities (including Editor Extensions' visible `ee-infinity-*` entities). Production and consumption cover exactly the ticks that ran, per quality and for all qualities, summed over all surfaces. The save itself is never modified.

### Global Options

| Option                   | Description                             | Default                      |
| ------------------------ | --------------------------------------- | ---------------------------- |
| `--factorio-path <PATH>` | An explicit path to the Factorio binary, or a script that starts it with its arguments | Auto-detected                |
| `--config <CONFIG>`      | Path to config file (also `BELT_CONFIG`) | `~/.config/belt/config.toml` |
| `--workspace-dir <DIR>`  | Where BELT keeps its mod folder and temporary run folders | OS data directory |
| `--sanitizer-mod <DIR>`  | Use a local belt-sanitizer mod folder instead of the mod portal release | mod portal |
| `--verbose`              | Debug logging, including every Factorio launch and its output | `false`                      |
| `--init-config`          | Initialize config directory             | n/a                          |
| `--version`              | Print version                           | n/a                          |

### Configuration

Every option can also come from the config file or an environment variable; flags beat environment variables, which beat the config file, which beats the defaults. Sections are `[global]`, `[benchmark]`, `[sanitize]` and `[blueprint]`, with the option names in snake case (`run_order`, `mods_dir`, `record_cpu`, …). Environment variables use a double underscore between section and key: `BELT_BENCHMARK__TICKS`, `BELT_GLOBAL__FACTORIO_PATH`. `belt --init-config` writes a commented config file with all defaults.

### Examples

#### Example 1: Basic Benchmarking

```bash
# Run 3 benchmarks per save for 6000 ticks each
belt benchmark ./my-saves --ticks 6000 --runs 3
```

#### Example 2: Pattern Filtering

```bash
# Benchmark only saves starting with "science"
belt benchmark ./my-saves --pattern "science*" --output ./science-results
```

#### Example 3: Custom Factorio Path

```bash
# Specify explicit Factorio binary location
belt --factorio-path /path/to/factorio benchmark ./my-saves
```

#### Example 4: Blueprint Testing

```bash
# Stamp each blueprint 100 times and let the game run 60 ticks before saving
belt blueprint ./blueprints ./base-save.zip --count 100 --buffer-ticks 60
```

#### Example 5: Item/Fluid Tracking

```bash
# Track specific items and fluids over 3600 ticks
belt sanitize ./saves --items "iron-plate,copper-plate" --fluids "water,crude-oil"
```

#### Charting Existing Data

BELT 4.0 no longer renders charts directly. Use the exported benchmark and verbose CSV files with external tooling such as `belt-charts`.

#### Appending Benchmark Data

Use `--append` to add a benchmark run to existing CSV output in the target `--output` directory:

```bash
belt benchmark ./my-saves --output ./benchmark-results --append
```

Append mode expects existing CSV headers to match the current BELT output format and the selected verbose metrics. `results.md` is regenerated from all rows in `results.csv`, and run indexes continue from the existing rows.

Existing files are checked before the first benchmark run starts, so a mismatch fails immediately instead of after the benchmark. Verbose metric headers are checked as each run finishes. Use the same `--compress` setting as the existing output: BELT refuses to write `<save>_verbose_metrics.csv` next to an existing `.csv.zst`, or the other way around. Appending to a `.csv.zst` adds a new zstd frame, so the existing data is never recompressed.

#### Custom Report Templates

`--template-path` renders the report with your own [Handlebars](https://handlebarsjs.com/) template, given the same variables as the built-in one (`templates/results.md.hbs`): `platform`, `factorio_version`, `date`, `ticks`, `runs`, `results` (per save: `save_name`, `avg_ms`, `min_ms`, `max_ms`, `avg_effective_ups`, `total_execution_time_ms`, `percentage_improvement`, `mimalloc`) and `amd_uprof`. A template named `report.html.hbs` writes `report.html` (bolding the best UPS with `<strong>`); other names write `results.md`.

### Advanced Usage

#### Best Practices

While `belt benchmark` offers sensible defaults, optimizing `--ticks` and `--runs` can refine your results. `--ticks` sets the simulation duration per run, while `--runs` determines the number of repetitions. Through testing, I've found that **fewer runs with more ticks** generally offers the most consistent UPS results for the shortest overall benchmark time, by reducing overhead from repeated Factorio launches. Experiment with these values for your specific saves to find the optimal balance for accuracy and speed.
However, for prolonged and thorough benchmarks, I recommend more runs in total, per save. This is because Factorio is deterministic, and when running BELT with verbose metrics, a "min" chart is generated. This chart is meant to combat any random noise that could slow down the Factorio benchmark, by only taking the fastest ticks of every run of a save.

#### AMD uProf Reports

BELT can include AMD uProf data in `results.md`, but it does not run uProf itself. Use a wrapper script as your `--factorio-path`, let that script run `AMDuProfCLI collect` and `AMDuProfCLI report`, and BELT will detect these lines in the benchmark output:

```text
Generated data files path: /path/to/session
Generated report file: /path/to/session/report.csv
```

If a report exists, BELT copies it to:

```text
<output>/uprof/<save_name>/run_<index>/report_<index>.csv
```

and renders the parsed tables in the Markdown report. If only a session path exists, BELT prints the session path and the `AMDuProfCLI report -i <session>` command to run manually.

Minimal wrapper shape:

```bash
#!/usr/bin/env bash
set -euo pipefail

FACTORIO_BIN="${HOME}/.factorio/bin/x64/factorio"
AMD_UPROF_CONFIG="${AMD_UPROF_CONFIG:-data_access}"
AMD_UPROF_VIEW="${AMD_UPROF_VIEW:-dc_focus}"
AMD_UPROF_ROOT="${AMD_UPROF_ROOT:-/tmp/belt-amduprof}"

mkdir -p "${AMD_UPROF_ROOT}"
session_parent="$(mktemp -d "${AMD_UPROF_ROOT}/session.XXXXXX")"
collect_log="$(mktemp "${session_parent}/collect.XXXXXX.log")"

AMDuProfCLI collect --config "${AMD_UPROF_CONFIG}" -o "${session_parent}" \
  "${FACTORIO_BIN}" "$@" 2>&1 | tee "${collect_log}"

generated_session="$(
  sed -n 's/^Generated data files path:[[:space:]]*//p' "${collect_log}" | tail -n 1
)"

AMDuProfCLI report -i "${generated_session:-${session_parent}}" --view "${AMD_UPROF_VIEW}"
```

The wrapper must pass all arguments through to Factorio, since BELT launches Factorio with its own `-c` and `--mod-directory` options.

For Factorio, `data_access` with `dc_focus` is a good starting point because it reports L1 data cache, DTLB, and refill-source counters. `hotspots` is useful for CPU time attribution, but it does not show cache misses. In the BELT report, start with the uProf summary row, then read the estimated L1 data cache summary for hit/miss rates derived from `L1_DC_ACCESSES_ALL.USER` and the demand refill source counters.

For deeper load-cache analysis, run your wrapper with:

```bash
AMD_UPROF_CONFIG=ibs AMD_UPROF_VIEW=ibs_op_ld belt benchmark ./saves --factorio-path ~/.local/bin/factorio
```

or use `AMD_UPROF_VIEW=ibs_op_ld_lat` for load miss latency. IBS collection usually requires `kernel.perf_event_paranoid <= 1` for non-root users:

```bash
sudo sysctl kernel.perf_event_paranoid=1
```

BELT renders IBS load reports as an `IBS Load Cache Summary` with L1 hit/miss rate, L2 hit rate, local/peer/remote cache hit rates, DRAM hit rate, and average L1 miss latency when those columns exist. The hottest functions/modules tables preserve AMD's raw counter columns. The copied CSV remains the source of truth for deeper analysis or AMD uProf GUI use.

#### Memory (mimalloc)

If the Factorio process prints mimalloc's heap statistics at exit (`heap stats: …`), BELT adds a "Memory (mimalloc)" section to the report with each run's committed and reserved memory, peak RSS, commit efficiency, pages, abandoned blocks and thread churn.

#### Exit codes

| Code | Meaning |
| ---- | ------- |
| `0`  | Every save succeeded. |
| `1`  | At least one save failed, or Ctrl-C stopped the run. Completed runs are still written, and each failed save is listed with the end of Factorio's log. |
| `2`  | The command could not run at all: a bad config, Factorio not found, a Factorio too old for the command, and so on. |

Scripts can tell "some saves failed" (`1`) apart from "nothing ran" (`2`). Before 4.4, every failure exited with `1`.

#### Verbose Metrics

Verbose metric files hold Factorio's values unchanged, in nanoseconds. With a list of metrics, a file holds exactly `tick,run,<your metrics>`; with `all`, every column Factorio reports is kept, including the empty last column Factorio's own header ends with. In `results.csv`, `max_ms` is then the highest `wholeUpdate` of the run (leaving out the first tick, a load spike), in milliseconds. An unknown metric name fails the command as soon as the first run that shows it is done.

Here are all the verbose-metrics that are available in Factorio 2.1:
`wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,turretTargetAcquisition,lightningUpdate,tileHeatingUpdate,pollutionUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate,LogisticRobot,ConstructionRobot,Inserter,Roboport,Loader,AssemblingMachine,AgriculturalTower,OldAgriculturalTower,Furnace,MiningDrill,FluidWagon,ArtilleryWagon,InfinityCargoWagon,CargoWagon,Locomotive,Character,Boiler,Generator,BurnerGenerator,Reactor,Lab,LandMine,ArtilleryFlare,ArtilleryProjectile,ArtilleryTurret,Beam,Car,SpiderVehicle,TemporaryContainer,CharacterCorpse,CombatRobot,CaptureRobot,Corpse,ElectricEnergyInterface,EnemySpawner,Explosion,FlameThrowerExplosion,FluidStream,FluidTurret,FlyingTextEntity,FusionGenerator,FusionReactor,Gate,HeatInterface,HighlightBoxEntity,InfinityContainer,InfinityPipe,ItemRequestProxy,OffshorePump,ParticleSource,PowerSwitch,Projectile,Pump,Valve,Radar,ProgrammableSpeaker,RocketSilo,RocketSiloRocket,CargoPod,SmokeWithTrigger,SpeechBubble,Sticker,Turret,AsteroidCollector,Asteroid,Thruster,SpiderUnit,Unit`

## Contributing

Any help is welcome. Whether you have never written a line of code, or simply don't know Rust. This is what the CI/CD pipeline is for!
Bug reports and feature requests can be submitting through GitHub Issues.

If you want to contribute, please open an issue to discuss the proposed changes before submitting a pull request.

### Standards

On every push a linter and formatter checks the code, so just write the code however you want and fix any errors that occur.

> [!NOTE]
> To do this locally, run `cargo fmt` and `cargo clippy -- -D warnings`

The tests that run a real Factorio are marked `#[ignore]`; run them with `cargo test -- --include-ignored`, with `BELT_TEST_SANDBOX` pointing at a scratch directory that holds a Factorio 2.1.25 headless install in `factorio-2.1.25/`, and `BELT_TEST_SANITIZER_MOD` at a belt-sanitizer checkout. The tests point `HOME` and `XDG_*` into that directory, so they never touch your own Factorio.

I follow the [Conventional Commits specification](https://www.conventionalcommits.org/) as a standard for my commit messages, I can only encourage you do the same.

## Credits

This was heavily inspired by abucnasty's videos. I wanted to make a more universal, cross-platform version of the existing ps1 script.
