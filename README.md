# BELT: Benchmark for Engine Limits & Throughput

![Crates.io Version](<https://img.shields.io/crates/v/belt?color=rgb(215%2C127%2C0)>)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/ci.yml?label=master)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/release.yml?label=release)

BELT is a comprehensive benchmarking and testing suite for Factorio, providing multiple output formats, and powerful analysis tools for optimizing your designs.

## Features

- [x] **Benchmarking** - Benchmark a single save or a whole directory
- [x] **Blueprint testing** - Automatically stamp and benchmark blueprints
- [x] **Cross-platform** - Works on Windows, macOS, and Linux
- [x] **Multiple output formats** - CSV and Markdown reports
- [x] **Pattern matching** - Filter save files by name patterns
- [x] **Async execution** - Fast parallel processing
- [x] **Verbose metrics support** - Per-tick CSV exports for detailed Factorio metrics
- [x] **Sanitizer** - Automatically parses and reports on [belt-sanitizer mod](https://mods.factorio.com/mod/belt-sanitizer) output
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

1. Factorio installed, BELT searches common installation paths, if none are found, please run with explicit `--factorio-path`.
2. Some save files to benchmark.
3. Rust if installing using cargo or building from source.

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

### Command Reference

#### `belt benchmark`

Run benchmarks on one or more save files.

**Arguments:**

- `[SAVES_DIR]` - The location of the save(s) to be benchmarked. Required unless `benchmark.saves_dir` is set in config.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--ticks <TICKS>` | How many ticks per run to run the benchmark for | `6000` |
| `--runs <RUNS>` | How many runs per save file | `5` |
| `--pattern <PATTERN>` | A pattern to match against when searching for save files in `<SAVES_DIR>` | `*` |
| `--output <OUTPUT_DIR>` | A directory to output the .csv and .md files to | `.` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for the benchmark| `--sync-mods` on each save file |
| `--run-order <RUN_ORDER>` | In which order to run the benchmarks. Available: `sequential`, `random`, `grouped` | `grouped` |
| `--verbose-metrics <VERBOSE_METRICS>` | Exports per-tick verbose metric CSVs for the selected metrics | `none` |
| `--strip-prefix <PREFIX>` | Strip a given prefix off of the save names | `none` |
| `--record-cpu` | Record CPU frequency samples during benchmark runs | `true` |
| `--append` | Append benchmark rows to existing output CSV files. Existing CSV headers must match the current output format and selected verbose metrics. | `false` |

#### `belt blueprint`

Stamp blueprints into a base save.

**Arguments:**

- `<BLUEPRINTS_DIR>` - Directory containing blueprint files.
- `<BASE_SAVE_PATH>` - Base save file to stamp blueprints into.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--count <COUNT>` | Number of times to stamp each blueprint. | **required** |
| `--buffer-ticks <BUFFER_TICKS>` | Ticks to wait between stamping operations. | **required** |
| `--mining-module-replacement <MINING_MODULE_REPLACEMENT>` | Module to insert into mining drills after ore marker modules have been interpreted. | `speed-module-3` |
| `--mining-module-replacement-quality <MINING_MODULE_REPLACEMENT_QUALITY>` | Quality of replacement modules inserted into mining drills. | `legendary` |
| `--bot-count <BOT_COUNT>` | Number of construction bots to use for building. | `0` |
| `--prefix <PREFIX>` | Prefix to add to generated save names. | `none` |
| `--pattern <PATTERN>` | Pattern to match against when searching for blueprint files. | `*` |
| `--output <OUTPUT_DIR>` | Directory to output generated saves. | `.` |
| `--mods-dir <MODS_DIR>` | Directory containing mods to use. | `--sync-mods` on each save file |

`belt blueprint` passes each blueprint string to the belt-sanitizer mod, which stamps it into the
base save before generating the benchmark save. For mining setups, the sanitizer creates ore patches
before reviving ghosts so drills can be built immediately. Ore selection is controlled by blueprint
markers in this order:

| Marker                         | Resource                                                                                                                     |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `stone-path` tile              | `stone`                                                                                                                      |
| `concrete` tile                | `iron-ore`                                                                                                                   |
| `hazard-concrete` tile         | `coal`                                                                                                                       |
| `refined-concrete` tile        | `copper-ore`                                                                                                                 |
| `refined-hazard-concrete` tile | `scrap`                                                                                                                      |
| `efficiency-module` request    | `stone`, `iron-ore`, `copper-ore`, `coal`, or `uranium-ore` for `normal`, `uncommon`, `rare`, `epic`, or `legendary` quality |
| `efficiency-module-2` request  | `calcite`, `tungsten-ore`, or `scrap` for `normal`, `uncommon`, or `rare` quality                                            |

Tile markers near a drill take precedence over module markers. After the resource is selected,
mining-drill module requests are replaced with `--mining-module-replacement` and
`--mining-module-replacement-quality`, so marker modules do not have to be the modules used in the
final save.

#### `belt sanitize`

Run the belt-sanitizer mod on save files to track item/fluid production and consumption.

**Arguments:**

- `[SAVES_DIR]` - The location of the save(s) to be sanitized. Required unless `sanitize.saves_dir` is set in config.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--pattern <PATTERN>` | A pattern to match against when searching for save files in `<SAVES_DIR>` | `*` |
| `--ticks <TICKS>` | How many ticks to run the sanitization for | `3600` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for the benchmark| `--sync-mods` on each save file |
| `--data-dir <DATA_DIR>` | If B.E.L.T. can't find your user data directory, pass it explicitely here. | `none` |
| `--items <ITEMS>` | A comma separated list of items to track. | `none` |
| `--fluids <FLUIDS>` | A comma separated list of fluids to track. | `none` |

### Global Options

| Option                   | Description                             | Default                      |
| ------------------------ | --------------------------------------- | ---------------------------- |
| `--factorio-path <PATH>` | An explicit path to the Factorio binary | Auto-detected                |
| `--config <CONFIG>`      | Path to config file                     | `~/.config/belt/config.toml` |
| `--headless`             | Run Factorio in headless mode           | `false`                      |
| `--verbose`              | Shows all debug statements              | `false`                      |
| `--init-config`          | Initialize config directory             | n/a                          |
| `--version`              | Print version                           | n/a                          |

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
# Stamp each blueprint 100 times with 60 tick buffer
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

Use `--append true` to add a benchmark run to existing CSV output in the target `--output`
directory:

```bash
belt benchmark ./my-saves --output ./benchmark-results --append true
```

Append mode expects existing CSV headers to match the current BELT output format and the selected
verbose metrics. Reports are regenerated from available CSV data, so details not stored in
`results.csv` may not be preserved.

### Advanced Usage

#### Best Practices

While `belt benchmark` offers sensible defaults, optimizing `--ticks` and `--runs` can refine your results. `--ticks` sets the simulation duration per run, while `--runs` determines the number of repetitions. Through testing, I've found that **fewer runs with more ticks** generally offers the most consistent UPS results for the shortest overall benchmark time, by reducing overhead from repeated Factorio launches. Experiment with these values for your specific saevs to find the optimal balance for accuracy and speed.
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

#### Verbose Metrics

Here are all the verbose-metrics that are available **PRE 2.1**:
`wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,lightningUpdate,tileHeatingUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate`

Here are all the verbose-metrics that are available **POST 2.1**:
`wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,turretTargetAcquisition,lightningUpdate,tileHeatingUpdate,pollutionUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate,LogisticRobot,ConstructionRobot,Inserter,Roboport,Loader,AssemblingMachine,AgriculturalTower,OldAgriculturalTower,Furnace,MiningDrill,FluidWagon,ArtilleryWagon,InfinityCargoWagon,CargoWagon,Locomotive,Character,Boiler,Generator,BurnerGenerator,Reactor,Lab,LandMine,ArtilleryFlare,ArtilleryProjectile,ArtilleryTurret,Beam,Car,SpiderVehicle,TemporaryContainer,CharacterCorpse,CombatRobot,CaptureRobot,Corpse,ElectricEnergyInterface,EnemySpawner,Explosion,FlameThrowerExplosion,FluidStream,FluidTurret,FlyingTextEntity,FusionGenerator,FusionReactor,Gate,HeatInterface,HighlightBoxEntity,InfinityContainer,InfinityPipe,ItemRequestProxy,OffshorePump,ParticleSource,PowerSwitch,Projectile,Pump,Valve,Radar,ProgrammableSpeaker,RocketSilo,RocketSiloRocket,CargoPod,SmokeWithTrigger,SpeechBubble,Sticker,Turret,AsteroidCollector,Asteroid,Thruster,SpiderUnit,Unit,`

## Contributing

Any help is welcome. Whether you have never written a line of code, or simply don't know Rust. This is what the CI/CD pipeline is for!
Bug reports and feature requests can be submitting through GitHub Issues.

If you want to contribute, please open an issue to discuss the proposed changes before submitting a pull request.

### Standards

On every push a linter and formatter checks the code, so just write the code however you want and fix any errors that occur.

> [!NOTE]
> To do this locally, run `cargo fmt` and `cargo clippy -- -D warnings`

I follow the [Conventional Commits specification](https://www.conventionalcommits.org/) as a standard for my commit messages, I can only encourage you do the same.

## Credits

This was heavily inspired by abucnasty's videos. I wanted to make a more universal, cross-platform version of the existing ps1 script.
`belt sanitize`'s settings.rs file was inspired by the [Typescript work of justarandomgeek](https://github.com/justarandomgeek/vscode-factoriomod-debug)
