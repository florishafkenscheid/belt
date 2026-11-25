# Changelog

## 4.4.1

### Fixed

- A single save file is accepted again wherever a saves directory is, as in 4.3.1
  (`belt sanitize path/to/save.zip`), and so is a single blueprint file for `blueprint`. 4.4.0
  refused both with "Not a directory".
- `--sync-mods` downloads work again. Factorio 2.1.25 downloads a save's missing mods while it
  syncs, as the identity in the run's own `player-data.json`, which was missing, so the mod portal
  answered 403 "Token mismatch". Each sync now gets your mod portal username and token (only those
  two fields) from Factorio's `player-data.json`. A failed sync prints its exit code as a number.
- Terminal output looks as it did in 4.3.1 again: levelled, coloured log lines on stdout (4.4.0
  printed its messages on stderr) and a progress bar with the time left for `benchmark` and
  `sanitize`. `--verbose` (or `verbose` in the config file or `BELT_GLOBAL__VERBOSE`) shows
  B.E.L.T.'s debug lines, including Factorio's own output. Output files are unchanged.

## 4.4.0

B.E.L.T. 4.4 is a rewrite that keeps the 4.x command line, config file and output files. It needs
belt-sanitizer 2.0.0, which it downloads from the mod portal by itself.

### Changed

- Factorio is no longer modified while B.E.L.T. runs. Every run uses its own temporary Factorio
  configuration and a separate B.E.L.T. mod folder, so your Factorio settings, saves, mod list and
  mods directory stay exactly as they were, even after a crash or Ctrl-C.
- `blueprint` works with headless Factorio. It builds every copy fully, places each copy's drills on
  their marker ore, and reports anything it could not place.
- Saves whose mods were made for an older Factorio still run. B.E.L.T. swaps each such mod for the
  newest version that works with your Factorio (installed, or downloaded from the mod portal) and
  prints a warning for every swap, such as `EditorExtensions 2.4.2 → 2.6.1`.
- `sanitize` production and consumption totals now cover exactly the ticks it ran. Before, they
  were roughly half of the real amounts.
- The belt-sanitizer mod is downloaded from the mod portal automatically, using the portal token
  in Factorio's `player-data.json`. `--sanitizer-mod <path>` uses a local copy instead.
- Factorio's version is checked before anything starts, and a command that needs a newer Factorio
  is refused with a message naming the version it needs.
- Exit codes: 1 when a save failed or Ctrl-C stopped the run, 2 when the command itself could not
  run (bad config, Factorio not found, and so on).

### Added

- `--workspace-dir` (`[global] workspace_dir`): where B.E.L.T. keeps its mod folder and temporary
  run folders.
- `--sanitizer-mod` (`[global] sanitizer_mod`): use a local belt-sanitizer folder.
- `--data-dir` now works for `benchmark` and `blueprint` too, not only `sanitize`.

### Breaking

- `--headless` is removed (and so is `headless` in the config file). Factorio now always runs in its
  own windowless modes, so the option has no meaning. A config file that still sets `headless` is
  refused with a message naming the key.
- `sanitize` needs Factorio 2.1.24 or newer, the first version with the launch option it uses.
