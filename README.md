# BELT: Benchmark for Engine Limits & Throughput
![Crates.io Version](https://img.shields.io/crates/v/belt?color=rgb(215%2C127%2C0))
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/ci.yml?label=master)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/release.yml?label=release)

BELT is a comprehensive benchmarking and testing suite for Factorio, providing multiple output formats, and powerful analysis tools for optimizing your designs.

## Features
- [x] **Benchmarking** - Benchmark a single save or a whole directory
- [x] **Blueprint testing** - Automatically stamp and benchmark blueprints
- [x] **Cross-platform** - Works on Windows, macOS, and Linux
- [x] **Multiple output formats** - CSV, Markdown reports, and SVG charts
- [x] **Pattern matching** - Filter save files by name patterns
- [x] **Async execution** - Fast parallel processing
- [x] **Verbose metrics support** - Per-tick charts and CSV exports for detailed Factorio metrics
- [x] **Sanitizer** - Automatically parses and reports on [belt-sanitizer mod](https://mods.factorio.com/mod/belt-sanitizer) output
- [x] **Smoothing and downsampling** - Configurable smoothing for charts to handle large datasets

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
#### `belt analyze`
Generate charts from existing benchmark CSV data.

**Arguments:*
- `<DATA_DIR>` - The location of the csv(s) to generate charts based off of.
- `<HEIGHT>` - The height of the generated charts in pixels.
- `<WIDTH>` - The width of the generated charts in pixels.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--smooth-window <SMOOTH_WINDOW>` | Add a smoothing effect to generated charts | `0` |
| `--verbose-metrics <VERBOSE_METRICS>` | Generates more charts based on the `--benchmark-verbose` factorio argument | `none` |
| `--max-points <MAX_POINTS>` | Max data points that the verbose charts can reach before being downsampled | `0` |

#### `belt benchmark`
Run benchmarks on one or more save files.

**Arguments:**
- `<SAVES_DIR>` - The location of the save(s) to be benchmarked.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--ticks <TICKS>` | How many ticks per run to run the benchmark for | `6000` |
| `--runs <RUNS>` | How many runs per save file | `5` |
| `--pattern <PATTERN>` | A pattern to match against when searching for save files in `<SAVES_DIR>` | `*` |
| `--output <OUTPUT_DIR>` | A directory to output the .csv and .md files to | `.` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for the benchmark| `--sync-mods` on each save file |
| `--run-order <RUN_ORDER>` | In which order to run the benchmarks. Available: `sequential`, `random`, `grouped` | `grouped` |
| `--verbose-metrics <VERBOSE_METRICS>` | Generates more charts based on the `--benchmark-verbose` factorio argument | `none` |
| `--strip-prefix <PREFIX>` | Strip a given prefix off of the save names | `none` |
| `--headless <HEADLESS>` | Whether or not to assume headless factorio | `false` |

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
| `--bot-count <BOT_COUNT>` | Number of construction bots to use for building. | `0` |
| `--prefix <PREFIX>` | Prefix to add to generated save names. | `none` |
| `--pattern <PATTERN>` | Pattern to match against when searching for blueprint files. | `*` |
| `--output <OUTPUT_DIR>` | Directory to output generated saves. | `.` |
| `--mods-dir <MODS_DIR>` | Directory containing mods to use. | `--sync-mods` on each save file |
| `--headless <HEADLESS>` | Whether or not to assume headless factorio | `false` |

#### `belt sanitize`
Run the belt-sanitizer mod on save files to track item/fluid production and consumption.

**Arguments:**
- `<SAVES_DIR>` - The location of the save(s) to be sanitized.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--pattern <PATTERN>` | A pattern to match against when searching for save files in `<SAVES_DIR>` | `*` |
| `--ticks <TICKS>` | How many ticks to run the sanitization for | `3600` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for the benchmark| `--sync-mods` on each save file |
| `--data-dir <DATA_DIR>` | If B.E.L.T. can't find your user data directory, pass it explicitely here. | `none` |
| `--items <ITEMS>` | A comma separated list of items to track. | `none` |
| `--fluids <FLUIDS>` | A comma separated list of fluids to track. | `none` |
| `--headless` | Whether or not to assume headless factorio | `false` |

### Global Options
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--factorio-path <PATH>` | An explicit path to the factorio binary | Auto-detected |
| `--verbose` | Shows all debug statements | `false` |

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

#### Example 5: Analyzing Existing Data
```bash
# Generate charts from existing benchmark data
belt analyze ./benchmark-results --height 600 --width 1200 --smooth-window 50
```

#### Example 6: Item/Fluid Tracking
```bash
# Track specific items and fluids over 3600 ticks
belt sanitize ./saves --items "iron-plate,copper-plate" --fluids "water,crude-oil"
```

### Advanced Usage
#### Best Practices
While `belt benchmark` offers sensible defaults, optimizing `--ticks` and `--runs` can refine your results. `--ticks` sets the simulation duration per run, while `--runs` determines the number of repetitions. Through testing, I've found that **fewer runs with more ticks** generally offers the most consistent UPS results for the shortest overall benchmark time, by reducing overhead from repeated Factorio launches. Experiment with these values for your specific saevs to find the optimal balance for accuracy and speed.
However, for prolonged and thorough benchmarks, I recommend more runs in total, per save. This is because Factorio is deterministic, and when running BELT with verbose metrics, a "min" chart is generated. This chart is meant to combat any random noise that could slow down the Factorio benchmark, by only taking the fastest ticks of every run of a save.

#### Verbose Metrics
Here are all the verbose-metrics that are available:
`wholeUpdate,latencyUpdate,gameUpdate,planetsUpdate,controlBehaviorUpdate,transportLinesUpdate,electricHeatFluidCircuitUpdate,electricNetworkUpdate,heatNetworkUpdate,fluidFlowUpdate,entityUpdate,lightningUpdate,tileHeatingUpdate,particleUpdate,mapGenerator,mapGeneratorBasicTilesSupportCompute,mapGeneratorBasicTilesSupportApply,mapGeneratorCorrectedTilesPrepare,mapGeneratorCorrectedTilesCompute,mapGeneratorCorrectedTilesApply,mapGeneratorVariations,mapGeneratorEntitiesPrepare,mapGeneratorEntitiesCompute,mapGeneratorEntitiesApply,spacePlatforms,collectorNavMesh,collectorNavMeshPathfinding,collectorNavMeshRaycast,crcComputation,consistencyScraper,logisticManagerUpdate,constructionManagerUpdate,pathFinder,trains,trainPathFinder,commander,chartRefresh,luaGarbageIncremental,chartUpdate,scriptUpdate`

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
