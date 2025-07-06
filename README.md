# BELT: Benchmark for Engine Limits & Throughput
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/ci.yml?label=master)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/florishafkenscheid/belt/release.yml?label=release)

> [!NOTE]
> This was heavily inspired by abucnasty's work.
> I wanted to make a more universal, cross-platform version of the existing ps1 script.

BELT is a wrapper for the `factorio --benchmark` command, to make it more user friendly, more efficient to use, and to generate templated markdown files with the gotten data.

## Features
- [ ] **Benchmarking** - Benchmark a single save or a whole directory
- [ ] **Cross-platform** - Works on Windows, macOS, and Linux
- [ ] **Multiple output formats** - CSV and Markdown reports
- [ ] **Pattern matching** - Filter save files by name patterns
- [ ] **Async execution** - Fast parallel processing

## Quick Start
```bash
# Install BELT
cargo install factorio-belt

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
cargo install factorio-belt
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
belt benchmark /path/to/saves --pattern "benchmark" --output /path/to/output/dir
```

### Command Reference
#### `belt benchmark`
**Arguments:**
- `<SAVES_DIR>` - The location of the save(s) to be benchmarked.

**Options:**
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--ticks <TICKS>` | How many ticks per run to run the benchmark for | `6000` |
| `--runs <RUNS>` | How many runs per save file | `5` |
| `--pattern <PATTERN>` | A pattern to match against when searching for save files in `<SAVES_DIR>` | `*` |
| `--output <OUTPUT_DIR>` | A directory to output the .csv and .md files to | `.` |
| `--mods-dir <MODS_DIR>` | A directory containing mods to be used for the benchmark | `--sync-mods` on each save file |
| `--run-order <RUN_ORDER>` | In which order to run the benchmarks. Available: `sequential`, `random`, `grouped` | `grouped` |
| `--verbose-charts` | Generates more charts based on the `--benchmark-verbose` factorio argument | `false` |

### Global Options
| Option | Description | Default |
| ------ | ----------- | ------- |
| `--factorio-path <PATH>` | An explicit path to the factorio binary | Auto-detected |
| `--verbose` | Shows all debug statements | `false` |

### Examples
#### Example 1: Basic Benchmarking
```bash
# Run a benchmark on the my-saves directory for 6000 ticks per run, and running each save file 3 times.
belt benchmark ./my-saves --ticks 6000 --runs 3
```

#### Example 2: Pattern Filtering
```bash
# Run a benchmark on the my-saves directory, only matching save files that start with "science" and outputting it to science-results/results.{csv,md}
belt benchmark ./my-saves --pattern science --output science-results
```

#### Example 3: Custom Factorio Path
```bash
# Run a benchmark on the my-saves directory, with an explicit path to the factorio binary
belt --factorio-path /path/to/factorio benchmark ./my-saves
```

#### Example 4: Specifying a mod list
```bash
# Run a benchmark on the my-saves directory and a mod directory
belt --factorio-path /path/to/factorio --mods-dir /path/to/mods benchmark ./my-saves
```

### Advanced Usage
While `belt benchmark` offers sensible default, optimizing `--ticks` and `--runs` can refine your results. `--ticks` sets the simulation duration per run, while `--runs` determines the number of repetitions. Through testing, I've found that **fewer runs with more tickS** generally offers the most consistent UPS results for the shortest overall benchmark time, by reducing overhead from repeated Factorio launches. Experiment with these values for your specific saevs to find the optimal balance for accuracy and speed.

## Contributing
Any help is welcome. Whether you have never written a line of code, or simply don't know Rust. This is what the CI/CD pipeline is for!
Bug reports and feature requests can be submitting through GitHub Issues.

If you want to contribute, please open an issue to discuss the proposed changes before submitting a pull request.

### Standards
On every push a linter and formatter checks the code, so just write the code however you want and fix any errors that occur.
> [!NOTE]
> To do this locally, run `cargo fmt` and `cargo clippy -- -D warnings`

I follow the [Conventional Commits specification](https://www.conventionalcommits.org/) as a standard for my commit messages, I can only encourage you do the same.
