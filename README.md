# reclean

`reclean` is short for "recursive clean".

A fast, safe Rust command-line utility for recursively removing files and directories matching glob patterns. Designed for cleaning development artifacts with multiple safety measures and performance optimizations.

## Features

- **Pattern Matching**: Include and exclude glob patterns with full wildcard support

- **Dependencies**: Optional removal of `node_modules`, `.venv` and `vendor`, matched only beside their lock file

- **Build artifacts**: Optional removal of build output, matched by project layout rather than by name

- **Safety First**: Path traversal protection, symlink guards, confirmation prompts, and dry-run mode

- **Performance**: Metadata caching, pre-compiled glob matchers, and optimized traversal

- **Statistics**: Optional breakdown of deletions by pattern with size reporting

- **Configuration**: `.reclean.toml` with automatic discovery (upward search + global fallback)

- **JSON Output**: Machine-readable output for scripting and automation

- **Shell Completions**: Generated completions for bash, zsh, fish, elvish, powershell

- **Error Handling**: Graceful error recovery with clear diagnostics; non-zero exit on failures

## Installation

```sh
# Install from crates.io
cargo install reclean

# Or build and install to /usr/local/bin
make install

# Or build release binary
cargo build --release
```

## Usage

```sh
% reclean --help
Safely remove files and directories matching a set of glob patterns.

Usage: reclean [OPTIONS]

Options:
  -p, --path <PATH>               Working directory [default: .]
  -g, --glob <GLOB>               Include glob pattern(s) (can specify multiple)
  -e, --exclude <EXCLUDE>         Exclude glob pattern(s) (can specify multiple)
  -c, --configfile [PATH]         Load config (searches upward, then ~/.config/reclean/)
  -w, --write-configfile          Write default '.reclean.toml' file
  -d, --dry-run                   Preview deletions without removing
  -y, --skip-confirmation         Skip confirmation prompt
  -s, --stats                     Display statistics by pattern
  -o, --older-than <DURATION>     Only remove files older than duration (e.g., "30d", "7d", "24h")
      --larger-than <SIZE>        Only remove targets of at least this size (e.g., "100M", "1.5G")
  -P, --progress                  Show progress bar during scanning
  -i, --include-symlinks          Include matched symlinks for removal
  -r, --remove-broken-symlinks    Remove broken symlinks
  -B, --build-artifacts           Also match build output at the top level of a project
  -D, --dependencies              Also match dependency trees beside their lock file
  -v, --verbose                   Increase verbosity (debug-level logging)
  -q, --quiet                     Suppress all output except errors
  -l, --list                      List default glob patterns
      --completions <SHELL>       Generate shell completions (bash, zsh, fish, elvish, powershell)
      --format <FORMAT>           Output format: text (default) or json
      --no-protect                Match inside protected (.git, .ssh, ...) and excluded-by-default (.venv) directories
  -h, --help                      Print help
  -V, --version                   Print version
```

### Examples

```bash
# Preview what would be deleted (dry-run)
reclean -d

# Remove with default patterns (requires confirmation)
reclean

# Custom patterns with multiple includes
reclean -g "*.log" -g "**/*.tmp"

# Also remove build output: ./target beside Cargo.toml, ./build beside CMakeLists.txt
reclean -d -B

# Exclude specific patterns
reclean -g "*.cache" -e "**/important.cache"

# Also remove dependency trees: ./node_modules beside package-lock.json, ...
reclean -d -D

# Only remove targets of 100 MiB or more
reclean -d -B -D --larger-than 100M

# Show statistics breakdown
reclean -s

# Only remove files older than 30 days
reclean -o 30d

# Remove broken symlinks
reclean -r

# Skip confirmation (use with caution)
reclean -y

# Use config file (auto-discovers .reclean.toml upward or ~/.config/reclean/)
reclean -c

# Use config file with CLI overrides
reclean -c --dry-run --stats

# Use explicit config file path
reclean -c configs/my-cleanup.toml

# JSON output for scripting
reclean -d --format json | jq '.summary'

# Quiet mode for scripting
reclean -y -q
```

## Default Patterns

With no `--glob`, reclean matches caches and OS debris: `__pycache__`, `*.pyc`, `*.pyo`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.coverage`, `.DS_Store`, `Thumbs.db`, and a few more. Shell and REPL history (`.bash_history`, `.python_history`) is user data and is left alone; name it with `-g` to remove it. `reclean -l` prints the full list. `--glob` replaces it.

Build output and dependency trees are not patterns. Their names (`build`, `target`, `vendor`, `node_modules`) are too ordinary to match safely by name alone, so they are matched by project layout under `-B` and `-D`. A name-only match remains available as a glob: `reclean -g "**/node_modules"`.

## Configuration

### Config File

Create a `.reclean.toml` file to persist your settings:

```bash
# Generate default config
reclean -w
```

Every key is optional. A missing key takes its default: `path = "."`, every flag `false`, no age or size filter, and the built-in pattern, exclude and protected lists. `patterns = []` matches nothing: to match only build output or dependency trees, set it alongside `build_artifacts` or `dependencies`, since omitting `patterns` adds the built-in list.

Example `.reclean.toml`:

```toml
path = "."
patterns = [
    "**/__pycache__",
    "**/*.pyc",
    "**/.DS_Store"
]
# Omit to keep the built-in list; set to [] to exclude nothing.
exclude_patterns = [
    "**/.venv",
    "**/venv",
    "**/important/**",
    "**/keep.pyc"
]
dry_run = false
skip_confirmation = false
include_symlinks = false
remove_broken_symlinks = false
stats_mode = true
build_artifacts = false
dependencies = false
# Only remove targets of at least this many bytes.
# larger_than_bytes = 104857600

# Omit to keep the built-in list; set to [] to disable protection.
protected_dirs = [".git", ".hg", ".svn", ".config", ".ssh", ".gnupg"]
```

### Config Discovery

When you run `reclean -c` (without a path), the tool searches for configuration in this order:

1. `.reclean.toml` in the current directory, then each parent directory upward

2. The global config:

   - Linux and macOS: `$XDG_CONFIG_HOME/reclean/config.toml` when `XDG_CONFIG_HOME` is an absolute path, else `~/.config/reclean/config.toml`.

   - macOS, if neither exists: `~/Library/Application Support/reclean/config.toml`, where 0.5.0 read it. reclean warns and names the preferred location.

   - Windows: `%APPDATA%\reclean\config.toml`.

You can also specify an explicit path: `reclean -c path/to/config.toml`.

CLI flags always override config file values (e.g., `reclean -c --dry-run` forces dry-run even if the config says `dry_run = false`).

Unknown keys are an error, so a misspelt `dry_run` stops the run instead of being ignored.

### Shell Completions

Generate shell completions for your shell:

```bash
# Bash
reclean --completions bash > ~/.bash_completions/reclean

# Zsh
reclean --completions zsh > ~/.zfunc/_reclean

# Fish
reclean --completions fish > ~/.config/fish/completions/reclean.fish
```

### JSON Output

Use `--format json` for machine-readable output:

```bash
reclean -d --format json | jq '.summary'
```

The document opens with `"schema": 1`, which changes when a field changes meaning or is removed, and `config`, the config file used or `null`. It then includes:

- `matches` - Array of matched items with `path`, `size`, `pattern`, `reason`, `type` (`directory`, `file` or `symlink`), and `restore` for a dependency tree

- `summary` - Total count, size (bytes and human-readable), dry-run flag

- `stats` - Per-pattern breakdown (count, size) when `--stats` is enabled

- `reasons` - Per-reason breakdown (count, size), cheapest to restore first: `pattern`, `broken-symlink`, `build-artifact`, `dependency`

- `failures` - Array of failed deletions with path and error message

- `warnings` - Array of paths that could not be read, with the error message

## Build Artifacts

`-B` / `--build-artifacts` matches build output in addition to the glob patterns. Build output is found by project layout, not by name: a directory qualifies when its name is paired with a marker file below, and both that marker and `.git` sit beside it.

| Directory | Marker |
|-|-|
| `build` | `CMakeLists.txt`, `meson.build`, `package.json`, `build.gradle`, `build.gradle.kts`, `pyproject.toml`, `setup.py`, `pubspec.yaml` |
| `dist` | `package.json`, `pyproject.toml`, `setup.py` |
| `target` | `Cargo.toml`, `pom.xml` |
| `.next`, `.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache` | `package.json` |
| `.gradle` | `build.gradle`, `build.gradle.kts` |
| `zig-out`, `zig-cache`, `.zig-cache` | `build.zig` |
| `.build` | `Package.swift` |
| `_build` | `mix.exs` |

`build` and `target` are ordinary names, so both conditions are required. The `.git` test pins the match to the top level of a project:

```
project/
  .git                     -> marks the project root
  CMakeLists.txt           -> marks a CMake project
  build/                   -> matched
  src/program/
    CMakeLists.txt         -> a subdirectory carries its own
    build/                 -> not matched: no .git beside it
```

A matched directory is one deletion target and is not entered, so its contents are neither walked nor counted. Protected directories and excludes are applied first: build output inside `.venv` stays.

The project must also be the outermost one below `--path`. A submodule or vendored checkout carries its own `.git` and marker, so if any directory between it and `--path` holds a `.git`, its build output is skipped. Name the nested project on `--path` to clean it directly.

Matches are attributed to the pattern `build-artifact` in `--stats` and `--format json`.

## Dependencies

`-D` / `--dependencies` matches a dependency tree when the lock file that pins it sits beside it. The restore command is printed beside each match and carried as `restore` in JSON output.

| Directory | Lock file beside it | Restored by |
|-|-|-|
| `.venv` | `uv.lock` | `uv sync` |
| `node_modules` | `package-lock.json`, `npm-shrinkwrap.json` | `npm ci` |
| `node_modules` | `yarn.lock` | `yarn install --immutable` |
| `node_modules` | `pnpm-lock.yaml` | `pnpm install --frozen-lockfile` |
| `node_modules` | `bun.lock`, `bun.lockb` | `bun install --frozen-lockfile` |
| `vendor` | `go.mod` | `go mod vendor` |

The marker is the lock, not the manifest: `package.json` and `pyproject.toml` carry version ranges, so only the lock names the tree the command puts back. A tree with no lock beside it is left alone. A lock reached through a symlink does not count.

The built-in `.venv` exclude does not apply here: it exists to skip scanning a virtualenv, and `-D` with a `uv.lock` asks for it whole. Excludes from `--exclude` or a config file still apply.

Two things are not preserved. Edits made inside a dependency tree are lost; patched `vendor/` source is the usual case. For Go, removing `vendor/` makes the build use the module cache or the network.

## Size Filter

`--larger-than` keeps only targets of at least the given size. A directory is judged by the total size of its contents. Sizes are bytes by default, or binary `K`, `M`, `G`, `T` (also `KiB` ... `TiB`). A fractional value is converted exactly and truncated toward zero; signs, exponents and lowercase suffixes are errors.

## Output and Confirmation

Each match is listed with its size, followed by a total. A run with more than one reason also prints a breakdown by reason.

The prompt takes a single keypress. `y` proceeds; anything else cancels. Keys typed before the prompt appears are discarded. When stdin is not a terminal, one character is read from it instead, so `echo y | reclean` works; end of input cancels.

Targets are removed in parallel. Results are reported in list order.

## Age Filter

`--older-than` keeps a target only when it is older than the duration. For a directory, the newest entry anywhere inside it sets its age, so an old `__pycache__` holding one new file is kept. Symlinks are aged by the link's own timestamp.

A target is also kept when its age cannot be shown: a timestamp in the future, a timestamp that cannot be read, or a subdirectory that cannot be listed.

## Exit Codes

| Code | Meaning |
|-|-|
| 0 | Success, nothing matched, cancelled, or dry run |
| 1 | `--path` cannot be read, a removal failed, or confirmation could not be read |
| 2 | Bad option or invalid configuration |
| 3 | The run completed, but part of the tree could not be read |

Status 3 is reported as a warning per path, and under `warnings` in JSON output. Matches beneath an unreadable directory are not considered.

## Safety Measures

### Protected directories

These directory names are never matched and never entered:

```
.git  .hg  .svn  .config  .ssh  .gnupg
```

They hold data whose loss is expensive and unrecoverable, and their contents also match ordinary cleaning patterns: a git object store holds files named like build artifacts. Protection is by name and covers any entry type, so the `.git` *file* that marks a submodule is protected too.

Two deliberate exceptions:

- A directory named on `--path` is entered. Pointing reclean at `.git` is a deliberate act, and silently doing nothing there would be its own trap.

- `--no-protect` disables the list for one run. In a config file, `protected_dirs` replaces it outright, so a project can protect names of its own.

### Default excludes

These patterns are excluded from every run, and an excluded directory is not entered:

```
**/.venv  **/venv
```

A virtualenv is rebuilt from a lockfile, so scanning one costs time for matches nobody wants: an installed environment holds `__pycache__` directories by the hundred.

This is an ordinary exclude, not protection. Three ways to clean a virtualenv anyway:

- Name it on `--path`, which exempts the root as protection does.

- Pass `--no-protect`, which drops the built-in excludes along with the protected names. Excludes named in a config file or on `--exclude` are kept.

- Set `exclude_patterns` in a config file, which replaces the defaults outright.

### Other measures

- Safe defaults with curated pattern list

- Dry-run mode to preview deletions (`-d`)

- Confirmation prompts (skippable with `-y`)

- Path traversal protection via canonicalization

  - All paths validated to be within the working directory

  - Protects against malicious patterns like `../../etc/passwd`

- Paths starting with `..` are automatically skipped

- Symlinks only removed with explicit `--include-symlinks` flag

- Each target is re-checked just before removal. It must still resolve inside the working directory and be the same file (device and inode on Unix) the scan found. The window between the check and the removal is narrowed, not closed: removal resolves the path again.

- Broken symlinks only removed with `--remove-broken-symlinks` flag

## Development

```bash
# Run tests
cargo test
# or
make test

# Run with clippy
cargo clippy -- -W clippy::all

# Format code
cargo fmt

# Build release
cargo build --release
```

## Testing

Comprehensive test suite with 69 tests:
- 18 integration tests (dry-run, deletion, directories, patterns, symlinks, security, age filtering, directory sizing)

- 7 protected directory tests (traversal, opt-out, configurability, root exemption)

- 1 relative-path traversal test

- 10 duration parsing tests (all units, edge cases)

- 4 default pattern and dependency table tests

- 9 glob matching and TOML serialization tests

- 7 config discovery tests (upward search, global fallback, edge cases)

- 5 size formatting tests (B through TiB)

- 2 JSON output structure tests

All tests use `tempfile` for safe temporary directory creation.

## Links

- [Stack Overflow reference](https://stackoverflow.com/questions/76797185/how-to-write-a-recursive-file-directory-code-cleanup-function-in-rust)

## License

MIT
