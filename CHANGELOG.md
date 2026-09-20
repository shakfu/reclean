# CHANGELOG

All notable project-wide changes will be documented in this file. Note that each subproject has its own CHANGELOG.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/) and [Commons Changelog](https://common-changelog.org). This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Types of Changes

- Added: for new features.

- Changed: for changes in existing functionality.

- Deprecated: for soon-to-be removed features.

- Removed: for now removed features.

- Fixed: for any bug fixes.

- Security: in case of vulnerabilities.

---

## [Unreleased]

### Fixed

- **Default excludes restored to `**/.venv` and `**/venv`.** 0.4.2 emptied `DEFAULT_EXCLUDES` without touching the README or the two tests that assert a virtualenv is pruned, so the suite was red and a default run scanned and deleted inside local virtualenvs. The 0.4.2 note below no longer describes the shipped behaviour.

- **Config-file mode applies `--path`, `--glob`, `--preset` and `--format json`.** `-c` dispatched to a helper that read only a subset of the flags, so `rclean -c --path other-dir` cleaned the directory named in the config file instead of the one named on the command line -- the wrong scope for a deletion tool, reported as success. Pattern resolution and job reporting are now shared with non-config mode, `--list` answers before any config is loaded, and `-w` / `--write-configfile` conflicts with `-c` rather than being silently dropped. `--path` now defaults to `.` at the point of use instead of in the parser, which is how config mode tells an explicit path from an unset one.

- **Traversal skips a directory whose path will not canonicalize.** The containment check read a `canonicalize()` error as proof the directory was inside the working directory, so a permission failure, or a directory replaced by a symlink mid-walk, let the scan descend and match outside the root. Such a directory is now skipped with a warning.

## [0.4.2]

## Changes

- Note that `.venv` and `venv` are not include in `DEFAULT_EXCLUDES` by default to be consistent with prior behaviour.

## [0.4.1]

### Added

- **`-B` / `--build-artifacts` removes build output.** The default patterns cannot express it: `build`, `dist` and `target` are ordinary directory names, and a glob for them would claim a source directory called `build` as readily as a CMake output tree. A directory is matched only when its name is paired with a marker file (`target` with `Cargo.toml` or `pom.xml`, `build` with `CMakeLists.txt`, `package.json`, `pyproject.toml`, ...) and both that marker and `.git` sit beside it. Requiring `.git` pins the match to the top level of a project, where a marker alone would not: a CMake subdirectory carries its own `CMakeLists.txt`, so `src/program/build` would qualify too. 25 pairs across C/C++, Rust, JavaScript, JVM, Python, Zig, Swift, Elixir and Dart; `--list` prints the directory names. Ported from cclean.

### Changed

- **`.venv` and `venv` moved from protected directories to (optional) default excludes.** Protection is for data whose loss is unrecoverable; a virtualenv is rebuilt from a lockfile, so it does not belong on a list users cannot override per run. If included in `DEFAULT_EXCLUDES`, it stays off the walk by default because scanning one is slow and matches thousands of items nobody wants. `exclude_patterns` now defaults to `["**/.venv", "**/venv"]`, and the protected list is `.git`, `.hg`, `.svn`, `.config`, `.ssh` and `.gnupg`. To clean a virtualenv: name it on `--path`, pass `--no-protect`, or set `exclude_patterns` in a config file.

- **An excluded directory is no longer entered.** The exclude check ran after the include match, so excluding a directory only stopped it from being deleted -- the walk still descended and reported every match inside as excluded, one line per item. Excludes are now checked first and prune the walk. A pattern aimed at files (`**/keep.pyc`) is unaffected; one aimed at a directory (`**/build`) now also spares its contents.

---

## [0.4.0]

### Fixed

- **Protected directories.** `.git`, `.hg`, `.svn`, `.venv`, `venv`, `.config`, `.ssh` and `.gnupg` are never matched and never entered. The default patterns reached inside all of them: on a tree with a `.pyc` file in each of `.git/objects`, `.venv/lib`, `.ssh` and `src`, rclean matched six items, including `.ssh/id_rsa.pyc` and `.git/objects/cached.pyc`. Avoiding that took an `--exclude` pattern per directory worth protecting, which is not something a user can be expected to get right before the first run.

  Protection is by name and covers any entry type, so the `.git` *file* marking a submodule is protected too. A directory named on `--path` is still entered, since pointing rclean at `.git` is deliberate. `--no-protect` disables the list for one run, and `protected_dirs` in a config file replaces it.

- **A matched directory no longer counts its own contents a second time.** The walk descended into a directory it had already matched, so every file inside was matched again as a separate item. On a tree of 1,800 `__pycache__` directories holding 16.82 MiB, the confirmation prompt reported 27,000 items and 33.65 MiB. Deletion was correct -- removing the directory made the later removals no-ops -- but the estimate a user reads before confirming was not. The walk now stops descending at a matched directory.

- **`--dry-run` no longer asks for confirmation.** A dry run removes nothing, and the prompt failed outright when stdin was not a terminal, so `-d` needed `-y` to run in any script or pipeline.

- **Log output goes to stderr.** Info-level lines were written to stdout ahead of the JSON document, so the pipeline documented in the README,
  `rclean -d --format json | jq '.summary'`, could not parse. Diagnostics on stderr
  and data on stdout also keeps `-q` from being structural.

- **Test fixture `tests/.drclean.toml` renamed to `tests/.rclean.toml`**, the name `test_toml_load` and `test_toml_table_from_file` read. `cargo test` exited non-zero on a clean checkout.

- **Directory sizing no longer follows symlinks.** A symlink is counted at its own size, matching what `remove_dir_all` deletes, and a symlink cycle inside a matched directory no longer recurses without end.

### Changed

- **The tree is walked in parallel**, and matched directories are sized in parallel, with one directory listing as the unit of work, so a single deep tree spreads across cores as well as many shallow ones. Results are sorted by path before reporting, which makes a run reproducible -- the previous order was whatever `readdir` returned.

- **Path-safety canonicalization runs once per directory** rather than once per match. The walk never follows symlinks, so an entry can only escape the working directory through a symlinked ancestor; the answer is a property of the parent directory, not of each entry in it.

- **The pattern that matched an entry is resolved only for `--stats` and `--format json`**, the two consumers of it, instead of on every match.

Together these take a dry run over a 64,821-entry tree from 746 ms to 73 ms.

### Removed

- **Dependencies on `fs_extra` and `walkdir`**, taking the runtime dependency count from 14 to 12. `fs_extra` was used for `get_size` alone, which followed symlinks; `walkdir` drove the serial walk that the parallel one replaces.

---

## [0.3.0]

### Changed

- **Renamed project from `rclean` to `rclean`** for crates.io publishing (the name `rclean` was already taken)

  - Package name: `rclean` -> `rclean`

  - Binary name: `rclean` -> `rclean`

  - Config file: `.rclean.toml` -> `.rclean.toml`

  - Global config directory: `~/.config/rclean/` -> `~/.config/rclean/`

  - Library crate imports: `use rclean::` -> `use rclean::`

- **Added doc comments to all public API items** for docs.rs documentation

  - Crate-level module documentation with quick-start example

  - `CleanError`, `CleanConfig`, `CleanConfigBuilder`, `CleaningJob`, `MatchedItem` structs and all public fields/methods

### Added

- **Published to crates.io**: `cargo install rclean` is now the primary installation method

- **CHANGELOG.md** included in the published package

---

## [0.2.2]

### Fixed

- **Critical: Spurious ENOENT failures when directory and child patterns overlap**: When glob patterns matched both a directory (e.g., `**/__pycache__`) and files inside it (e.g., `**/*.pyc`), `remove_dir_all` on the directory would recursively delete all children. Subsequent attempts to delete those same children individually produced hundreds of "No such file or directory" errors. `execute_deletion()` now tracks which directories have been recursively removed and skips any targets that are descendants of an already-deleted directory.

### Added

- **Regression test**: `test_no_failures_when_dir_and_children_both_match` covering the overlapping directory/child pattern scenario (55 tests total)

---

## [0.2.1]

### Added

- **Builder Pattern**: `CleanConfig::builder()` fluent API replaces direct struct construction

  - All fields configurable via chained methods (e.g., `.path(".")..dry_run(true).build()`)

  - Eliminates need for `#[allow(clippy::too_many_arguments)]`

- **Config Discovery**: `rclean -c` (without a path) now searches for config automatically

  - Searches upward from the current directory for `.rclean.toml`

  - Falls back to global config at `~/.config/rclean/config.toml`

  - Explicit path still supported: `rclean -c path/to/config.toml`

  - New public functions: `find_config_upward()`, `global_config_path()`, `discover_config()`

- **CLI Flag Overrides**: When using `-c`, CLI flags now override config file values

  - `--dry-run`, `--stats`, `--progress`, `--exclude`, `--older-than`, etc.

  - Example: `rclean -c --dry-run` forces dry-run even if config says `dry_run = false`

- **Pattern Presets**: Named pattern groups via `--preset` flag

  - Available presets: `common`, `python`, `node`, `rust`, `java`, `c`, `go`, `all`

  - Combinable: `--preset python --preset node`

  - Combinable with custom patterns: `--preset python -g "**/*.log"`

  - List preset contents: `rclean -l --preset python`

- **JSON Output**: `--format json` for machine-readable structured output

  - Includes `matches`, `summary`, `stats`, and `failures` sections

  - Human-readable sizes in summary and stats

  - Suppresses text logging when active

- **Shell Completions**: `--completions <SHELL>` generates completions

  - Supports bash, zsh, fish, elvish, powershell

  - Uses `clap_complete` crate

- **Human-Readable Sizes**: All size output now uses IEC binary units

  - Format: B, KiB, MiB, GiB, TiB

  - Applied to statistics, summary, and JSON output

  - Public `format_size()` function available in library API

- **Verbose/Quiet Modes**: `--verbose` (`-v`) and `--quiet` (`-q`) flags

  - Verbose enables debug-level logging

  - Quiet suppresses all output except errors

- **New Dependencies**: `clap_complete`, `serde_json`, `dirs`

- **New Tests** (54 total, up from 19):

  - 7 config discovery tests (upward search, global fallback, edge cases)

  - 5 size formatting tests (B through TiB)

  - 10 duration parsing tests (all units, edge cases)

  - 9 preset resolution tests (all presets, deduplication, unknown handling)

  - 2 JSON output tests (structure, empty results)

  - 2 age-based filtering integration tests

### Changed

- **Architecture**: Split `CleaningJob` into `CleanConfig` (serializable) + `CleaningJob` (runtime)

  - `CleanConfig` holds all configuration with serde support

  - `CleaningJob` holds runtime state (targets, stats, counters)

  - Clean separation of concerns

- **Pre-compiled Matchers**: Glob patterns compiled once in `build_globsets()`

  - `PatternMatchers` type alias: `Vec<(String, GlobMatcher)>`

  - Eliminates per-entry glob recompilation for stats attribution

- **Progress Bar + Output**: Progress bar no longer suppresses `--stats` or match logging

  - Uses `pb.println()` to interleave output with spinner

  - All match/exclude/delete messages route through progress-aware logging

- **Counter Type**: Changed `counter` from `i32` to `usize`, stats values from `(i32, u64)` to `(usize, u64)`

- **Memory**: `execute_deletion()` uses `std::mem::take` instead of `.clone()` for targets

- **Config File Path**: `--configfile` (`-c`) now accepts optional path argument

  - `-c` alone triggers config discovery

  - `-c path/to/config.toml` uses specified config file

- **Non-Zero Exit Code**: Process exits with code 1 when any deletions fail

- **Default Patterns**: Now defined as `common` + `python` presets combined (deduplicated)

### Fixed

- **Dry-run Default Confusion**: `CleanConfig::default()` now sets `dry_run = false`

  - Previously inconsistent between Default impl and CLI behavior

  - CLI `-d` flag still works as expected for explicit dry-run

- **License Contradiction**: Fixed README stating "Unlicense" while Cargo.toml and LICENSE file specified MIT

  - README now correctly states MIT

- **Makefile**: Added `test` target (`cargo test`)

---

## [0.2.0]

### Added

- **Progress Bar**: New `--progress` (`-P`) flag to show real-time scanning progress

  - Displays elapsed time and items scanned

  - Updates every 100 items for efficiency, and doesn't display INFO log

  - Shows final summary on completion

  - Uses `indicatif` crate for smooth spinner animation

- **Age-Based Filtering**: New `--older-than` (`-o`) flag to only remove old files

  - Example: `rclean -g "*.log" --older-than "30d"`

  - Supports time units: s (seconds), m (minutes), h (hours), d (days), w (weeks)

  - Checks file modification time against threshold

  - Skips files newer than specified duration

- **Error Tracking**: Failed deletions are now tracked and reported

  - `failed_deletions` field tracks all deletion errors

  - Error summary displayed at end of operation

  - Shows path and error message for each failure

  - Helps identify permission issues and locked files

- **Exclude Patterns Feature**: New `--exclude` (`-e`) flag to skip files matching specific patterns

  - Example: `rclean -g "*.log" --exclude "important.log"`

  - Supports multiple exclude patterns

  - Works with config file via `exclude_patterns` field

- **Statistics Mode**: New `--stats` (`-s`) flag to display breakdown of matches by pattern

  - Shows count and total size for each pattern

  - Sorted by match count (descending)

  - Useful for understanding what's being cleaned

- **Custom Error Handling**: Comprehensive `CleanError` enum for better error messages

  - IoError, GlobError, PathTraversal, PermissionDenied, ConfigError variants

  - Proper `Display` and `Error` trait implementations

  - Graceful error propagation throughout codebase

- **Integration Tests**: 10 comprehensive integration tests using tempfile

  - Tests for dry-run, actual deletion, directory removal, multiple patterns

  - Tests for broken symlinks, invalid patterns, size calculation

  - Tests for path traversal protection, exclude patterns, statistics mode

- **Dependencies**: Added `indicatif = "0.17"` for progress bars, `tempfile = "3"` for testing

### Changed

- **Major Refactoring**: Extracted `run()` method into focused helper methods

  - `build_globsets()` - Constructs include and exclude glob matchers

  - `should_process()` - Path validation and security checks

  - `find_matching_pattern()` - Pattern identification for statistics

  - `collect_targets()` - Directory traversal and matching

  - `handle_matched_entry()` - Entry processing with stats tracking

  - `execute_deletion()` - Batch file removal

  - `display_stats()` - Statistics reporting

  - Reduced `run()` from 78 lines to 44 lines (43% reduction)

- **Performance Optimization**: Changed `targets` from `Vec<DirEntry>` to `Vec<(PathBuf, Metadata)>`

  - Metadata cached during collection, eliminating redundant syscalls

  - Files use direct `metadata.len()` instead of `get_size()` for instant calculation

  - ~2-3x performance improvement for large directory trees

- **CleaningJob Constructor**: Now accepts `exclude_patterns` and `stats_mode` parameters

  - Note: This is a breaking change to the API

- **Error Handling**: All functions now return `Result<T>` instead of panicking

  - Main function exits cleanly with exit code 1 on errors

  - Removed all `unwrap()` and `expect()` calls in favor of proper error handling

### Fixed

- **Critical: Skip Confirmation Logic**: Fixed bug where entries were added to targets after deletion

  - When `skip_confirmation` was true, code would delete entries AND add them to targets list

  - Now properly uses `if/else` blocks to prevent double-processing

  - Same fix applied to broken symlink removal flow

- **Critical: Size Double-Counting**: Fixed inflated size calculations

  - Previously used `get_size()` for all entries, causing recursive counting

  - Now uses `metadata.len()` for files and `get_size()` only for directories

  - Eliminates 2-10x size inflation for directory-heavy cleanups

- **Critical: Test Schema Mismatch**: Fixed test struct missing `include_symlinks` and `remove_broken_symlinks` fields

  - Test deserialization now works correctly with real config files

- **Dry-Run Bug**: Fixed `dry_run` being ignored when `skip_confirmation` is true

  - Now properly checks both flags before deletion

- **Broken Symlink Counter**: Fixed counter not being incremented for broken symlinks

- **Duplicate Code**: Optimized `remove_entry()` to eliminate duplicate removal logic

  - Combined file and symlink removal (both use `remove_file()`)

- **Typo**: Fixed "deerialize" -> "deserialize" in error message

### Security

- **Path Traversal Protection**: Added canonicalization checks to prevent directory traversal attacks

  - Base path is canonicalized at start of operation

  - All entry paths are validated to be within base directory

  - Paths outside working directory are skipped with warnings

  - Protects against malicious patterns like `../../etc/passwd`

- **Config File Safety**: Improved validation of `.rclean.toml` contents

  - Better error messages for malformed configs

  - Pattern validation before execution

---

## [0.1.x]

- updated dependencies

## [0.1.3]

- Update dependencies and added `.ropeproject` as a cleanup pattern

## [0.1.2]

- Fixed size reporting which was faulty in earlier versions.

## [0.1.1]

- Added initial rclean code
