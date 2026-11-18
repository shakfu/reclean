# reclean behavioural inventory

Subject: repo `~/projects/reclean`, Cargo version 0.6.0 plus [Unreleased] commits (HEAD 6281e10).
All [observed] runs used `target/debug/reclean` built from HEAD, not the installed 0.6.0 binary.
The installed 0.6.0 predates the [Unreleased] output changes (CHANGELOG.md:31-39).
Dependency versions from Cargo.lock: clap 4.6.7, globset 0.4.20, dialoguer 0.12.0, indicatif 0.18.6, toml 0.9.
Citations: `main.rs` = src/main.rs, `lib.rs` = src/lib.rs, `constants.rs`, `remove.rs`, `file_id.rs`.

## 1. Invocation

No subcommands, no positional arguments. Parser is clap derive (main.rs:23-113).

| Short | Long | Arg | Default | Meaning | Cite |
|-|-|-|-|-|-|
| -p | --path | `<PATH>` | `.` | root directory | main.rs:26-28, 387 |
| -g | --glob | `<GLOB>` | default list | include pattern; repeatable, one value per occurrence | main.rs:30-32 |
| -e | --exclude | `<EXCLUDE>` | none (plus built-ins) | exclude pattern; repeatable | main.rs:34-36 |
| -c | --configfile | `[<PATH>]` optional value | off | load config; bare `-c` (or `-c ""`) discovers | main.rs:38-40, 251 |
| -w | --write-configfile | flag | off | write `.reclean.toml` in cwd | main.rs:42-44 |
| -d | --dry-run | flag | off | list only | main.rs:46-48 |
| -y | --skip-confirmation | flag | off | no prompt | main.rs:50-52 |
| -i | --include-symlinks | flag | off | matched symlinks become targets | main.rs:54-56 |
| -r | --remove-broken-symlinks | flag | off | broken symlinks become targets | main.rs:58-60 |
| -B | --build-artifacts | flag | off | see section 7 | main.rs:62-64 |
| -D | --dependencies | flag | off | see section 8 | main.rs:66-68 |
| -s | --stats | flag | off | per-pattern statistics | main.rs:70-72 |
| -o | --older-than | `<DURATION>` | none | age filter | main.rs:74-76 |
| (none) | --larger-than | `<SIZE>` | none | size filter | main.rs:78-80 |
| -P | --progress | flag, hidden | off | no-op, accepted for compatibility | main.rs:82-84 |
| (none) | --no-protect | flag | off | clears protected list and built-in excludes | main.rs:86-88 |
| -v | --verbose | flag | off | log level Debug | main.rs:90-92, 424-430 |
| -q | --quiet | flag | off | log level Error, progress off | main.rs:94-96, 299, 396 |
| (none) | --completions | `<SHELL>` | none | bash, elvish, fish, powershell, zsh | main.rs:98-100 |
| (none) | --format | `<FORMAT>` | text | `text` or `json` (case-sensitive) | main.rs:102-104 |
| (none) | --color | `<WHEN>` | auto | `auto`, `always`, `never` | main.rs:106-108 |
| -l | --list | flag | off | print built-in lists | main.rs:110-112 |
| -h | --help | flag | | clap help | clap |
| -V | --version | flag | | prints `reclean 0.6.0` [observed] | clap |

- About line in help is the Cargo description: `Safely remove files and directories matching a set of glob patterns.` [observed] (Cargo.toml description; main.rs:24).
- Value syntax: `-g X`, `-gX`, `-g=X`, `--glob X`, `--glob=X` all work [observed]. Short flags combine: `-dsy` [observed].
- Single-value options repeated: error `error: the argument '--path <PATH>' cannot be used multiple times`, exit 2 [observed].
- Missing value: `error: a value is required for '--path <PATH>' but none was supplied`, exit 2 [observed].
- Positional or stray value: `error: unexpected argument 'foo' found` + usage, exit 2 [observed].
- `--`: no positionals exist, so anything after `--` is `unexpected argument`, exit 2 [observed: `-d -- -g`]. `prescan_color` stops at `--` (main.rs:184).
- Values starting with `-` are parsed as flags: `-o -5d` gives `error: unexpected argument '-5' found`, exit 2; `--larger-than=-1` reaches the parser and gives `error: Configuration error: Invalid size '-1'`, exit 2 [observed].
- Bad enum value: `error: invalid value 'JSON' for '--format <FORMAT>'` then `  [possible values: text, json]`, exit 2 [observed]. Typo option: includes `  tip: a similar argument exists: '--color'` [observed].
- Conflicts: only `-w` vs `-c` (main.rs:43): `error: the argument '--write-configfile' cannot be used with '--configfile [<PATH>]'`, exit 2 [observed].
- No conflict declared for `-q` with `-v` (`-q` wins, main.rs:424-430) [observed exit 0], `-d` with `-y` (dry run wins, lib.rs:1322-1334), `-l` with anything.
- Dispatch order: `--completions` first, then `--list`, then `-c`, then filter parsing, then `-w`, then run (main.rs:336-412). So `-l -c nonexist` prints the list and ignores `-c` [observed]; `--completions zsh -l` prints completions [observed].
- `-o` and `--larger-than` parse errors exit 2 before any walk (main.rs:366-373; in -c mode after the config loads, main.rs:317-322).
- Duration error strings (lib.rs:91-139) [observed]:
  - `Configuration error: Duration cannot be empty`
  - `Configuration error: Invalid duration '5': must be a number followed by a unit (s, m, h, d, w)`
  - `Configuration error: Invalid duration unit 'x'. Use 's', 'm', 'h', 'd', or 'w'`
  - `Configuration error: Invalid number in duration: +5`
  - `Configuration error: Duration '<trimmed>' is too large` (checked_mul overflow, lib.rs:136-138) [source]
- Size error strings (lib.rs:146-181) [observed]: `Configuration error: Invalid size '<input>'`, `Configuration error: Size '<trimmed>' is too large`.
- All runtime errors print as `error: <Display>` on stderr (main.rs:446). Display prefixes: `IO error: `, `Glob pattern error: `, `Configuration error: ` (lib.rs:58-66).

## 2. Root

- Root text = `--path` or config `path`, default `.` (main.rs:387, lib.rs:362). Relative root resolves against the process cwd, also when set in a config file (README.md:195) [observed in t5].
- `base_path = canonicalize(root)`; failure gives `error: IO error: Invalid path 'nope': No such file or directory (os error 2)`, exit 1 (lib.rs:1263-1268) [observed].
- Root is never a target: only entries listed from directories are considered (lib.rs:1401-1404, 1422). `-p __pycache__ -g '*'` lists only its children [observed].
- Root is exempt from protected names and excludes (lib.rs:1402-1404): `-p venv` cleans inside a venv [observed]; `-p .git` likewise [source; tests/test_protected_dirs.rs:144].
- Root symlink to directory: walked as its target; the link itself is never a target (lib.rs:1414-1420) [observed `-p rootlink`]. Paths keep the link spelling.
- Root is a regular file (or symlink to file): no walk, no output, exit 0 (lib.rs:1416-1423) [observed `-p file.pyc`].
- Root missing or a broken symlink: canonicalize fails, exit 1 with `Invalid path` message [observed].
- `symlink_metadata(root)` failure after canonicalize: `IO error: Cannot read "<root>": <err>`, exit 1 (lib.rs:1405-1411) [source].
- Patterns see root-relative paths (section 3), so a root whose own text matches a pattern does not match (lib.rs:1016-1018; CHANGELOG.md:83).

## 3. Patterns

- Default list, in order (constants.rs:95-110) [observed via `-l`]:
  `**/.DS_Store`, `**/Thumbs.db`, `**/__pycache__`, `**/.coverage`, `**/.mypy_cache`, `**/.pylint_cache`, `**/.pytest_cache`, `**/.ruff_cache`, `**/.rumdl_cache`, `**/.pyscn`, `**/.ropeproject`, `**/pip-log.txt`, `**/*.pyc`, `**/*.pyo`.
- Dialect: `globset` `Glob::new` with default options (lib.rs:1356, 1367) [source; behaviour observed]:
  - `*` matches any sequence including `/` (literal_separator off). `-g 'a/*'` matched `a/__pycache__` and `a/b.pyc`; `*.pyc` matches at any depth [observed; README.md:132].
  - `**/x` also matches `x` at the top level [observed: `**/venv` excluded `./venv`].
  - `?` one char, `[ab]` class, `{A,a}` alternation [observed: `{A,a}.PYC`, `[ab]/b.py?`]. Unclosed class: `error: Glob pattern error: error parsing glob 'a[': unclosed character class; missing ']'`, exit 2 [observed].
  - Case-sensitive: `**/*.PYC` matched only `A.PYC` [observed].
  - Dotfiles: no special treatment; `*.pyc` matched `.hidden.pyc` [observed].
  - Backslash: globset escapes on Unix; on Windows `\` is a separator [source: globset default `backslash_escape` is platform-dependent; inference, not tested].
  - Empty pattern `""` matches only the empty path, so it matches nothing [observed with `-e ''`].
- Match subject: entry path with the walk root stripped (`path.strip_prefix(root)`), e.g. `src/main.c` (lib.rs:1016-1018). Patterns containing `/` are anchored at the root: `-g 'src/*.c'` matched `src/main.c`; `-g b.pyc` did not match `a/b.pyc` [observed].
- Every entry type is tested: files, directories, symlinks, fifos (lib.rs:996-1086).
- Combination:
  - CLI: `-g` replaces the defaults entirely (main.rs:376). Multiple `-g` are OR.
  - Config: key `patterns` replaces defaults; key omitted means defaults (lib.rs:320); `patterns = []` matches nothing (tests/test_config_defaults.rs:36).
  - Config + CLI: `-g` replaces the config list (main.rs:326-328).
- Attribution for stats/JSON: first pattern in list order whose matcher matches (lib.rs:887-894). Only computed when `-s` or JSON (lib.rs:1073-1078).

## 4. Excludes

- Default excludes: `**/.venv`, `**/venv` (constants.rs:29) [observed `-l`].
- Tested against the same root-relative path as patterns (lib.rs:1024-1025).
- Excluded entry is neither matched nor entered (pruning) (lib.rs:1020-1029). `-e keep` pruned `keep/` and its contents; `-e keep.pyc` excluded only top-level `keep.pyc`; `-e '**/keep.pyc'` excluded at any depth [observed].
- Excludes are case-sensitive (`-e KEEP` excluded nothing) [observed].
- Order of checks per entry: protected name, then configured excludes, then `-D` dependency check, then built-in excludes, then broken symlink, then pattern, then `-B` (lib.rs:1002-1064).
- Two exclude sets: patterns textually equal to a DEFAULT_EXCLUDES string form the "built-in" set; all others the "configured" set (lib.rs:1362-1374). Only the configured set applies before `-D`.
- Combination:
  - CLI: `-e` appends to the built-ins (main.rs:379-384).
  - `--no-protect` (CLI): built-ins dropped; `-e` still applies (main.rs:379-384).
  - Config: `exclude_patterns` replaces built-ins; omitted means built-ins (lib.rs:322-324, 364).
  - Config + CLI: `-e` appends (main.rs:314-316); `--no-protect` removes entries textually equal to a built-in (main.rs:306-313).
- `-v` logs `debug: Excluded: "./venv"` to stderr [observed].

## 5. Protected directories

- List: `.git`, `.hg`, `.svn`, `.config`, `.ssh`, `.gnupg` (constants.rs:15) [observed `-l`].
- Exact basename comparison (OsStr equality), case-sensitive, any entry type: files and symlinks with these names too (lib.rs:1002-1011). A file `.hg` was protected; `.git.pyc` was not [observed].
- Protected entries are neither matched nor entered, even if a pattern names them: `-g '**/.git' -g '**/.hg'` printed nothing [observed].
- Checked before excludes, patterns, `-B`, `-D` (lib.rs:1002).
- Override: `--no-protect` empties the list for the run (main.rs:401-405, 307). `-v` logs `debug: Protected, skipping: "<path>"` (lib.rs:1009).
- Config `protected_dirs` replaces the list; `[]` disables (lib.rs:342-344).
- Root is exempt (section 2).

## 6. Walk

- Parallel work queue, one directory listing per work item, threads = `available_parallelism()` (lib.rs:545-622).
- Symlinks: never followed for descent. Descent only for `file_type().is_dir()` of the unfollowed entry (lib.rs:1013, 1063).
- Symlink matched by a pattern: skipped unless `-i` (lib.rs:1066-1069). With `-i`, link becomes target of type `symlink`, size = lstat length (link text length) [observed `link__pycache__ 16 B`]. Symlink to a directory shows without trailing `/` [observed].
- Broken symlinks: with `-r`, any symlink whose `stat` fails (any error, not only ENOENT) is a target with reason `broken-symlink`, regardless of patterns or `-i` (lib.rs:1046-1050) [observed]. Without `-r` and with `-i`, a broken link matching a pattern is a `pattern` target [observed `broken.pyc` under `-i`].
- Directory containment: each directory is canonicalized before listing and must start with `base_path`; otherwise warning `resolves outside the working directory` and skipped (lib.rs:943-960).
- Other filesystems: no device check; mount points are crossed [source: no st_dev test in lib.rs:943-1086].
- A matched directory is one target and is not entered (lib.rs:1082-1085). Hence no nested targets (lib.rs:1570-1572). `-g '*.pyc'` found `a/__pycache__/x/f.pyc` only because `__pycache__` did not match [observed].
- Unreadable entries: directory that fails `read_dir`, entry read error, `file_type` error, `canonicalize` error, `symlink_metadata` error at collect: each recorded as a warning `warning: Cannot read "<path>": <err>` on stderr, the subtree skipped (lib.rs:639-645, 916-919, 956-985, 1091-1096). Any warning makes exit 3 unless a removal failed (main.rs:237-242) [observed].
- End-of-run summary: `warning: N path(s) could not be read; matches beneath them were not considered` (text mode only, lib.rs:1336-1341) [observed].
- Ordering: targets sorted by `Path` ordering (component-wise, byte order per component) after the walk (lib.rs:1433-1435). Observed order: `.DS_Store`, `.hidden.pyc`, `a/__pycache__/`, `a/b.pyc`, `ctl...`, `we\\ird.pyc`.
- Per-path warnings are emitted in nondeterministic thread order [observed: three git warnings in arbitrary order].

## 7. Build artifacts (-B)

- Pairs (constants.rs:45-80); distinct names sorted for `-l` (constants.rs:83-88):

| Dir | Marker(s) |
|-|-|
| `build` | `CMakeLists.txt`, `meson.build`, `package.json`, `build.gradle`, `build.gradle.kts`, `pyproject.toml`, `setup.py`, `pubspec.yaml` |
| `target` | `Cargo.toml`, `pom.xml` |
| `dist` | `package.json`, `pyproject.toml`, `setup.py` |
| `.next`, `.nuxt`, `.svelte-kit`, `.turbo`, `.parcel-cache` | `package.json` |
| `.gradle` | `build.gradle`, `build.gradle.kts` |
| `zig-out`, `zig-cache`, `.zig-cache` | `build.zig` |
| `.build` | `Package.swift` |
| `_build` | `mix.exs` |

- Rule (lib.rs:797-836), all required:
  1. Entry is a real directory (unfollowed) (lib.rs:1057).
  2. Basename is in the table.
  3. `parent/.git` exists, any type (`Path::exists`, follows symlinks) (lib.rs:811).
  4. A marker paired with this name is a file in the parent (`is_file`, follows symlinks) (lib.rs:815-820).
  5. Outermost: no `.git` (lstat, any error but NotFound counts as present) in any ancestor of the parent up to and including the root (lib.rs:822-834). Exception: project == root skips the loop.
  6. No pattern matched first (lib.rs:1052-1055); a pattern match reports reason `pattern`.
  7. Not git-tracked (below).
- [observed] `t4/proj` repo: `build/`, `dist/`, `target/` matched from both `t4` and `proj`; `sub/build` (no `.git` beside) and `sub/inner/target` (nested repo) not matched; `-p proj/sub/inner` matched `target/`.
- Submodule: `.git` file counts for 3 and 5 (lib.rs:810; tests/test_build_artifacts.rs:132).
- Git-tracked guard `git_tracks` (lib.rs:840-870):
  - Skip git (not tracked) unless some ancestor of canonical(parent) has `.git` (lstat, non-NotFound counts) (lib.rs:845-851).
  - Runs `git --literal-pathspecs -C <parent> ls-files -z -- <name>`, stdin null, with `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` removed (lib.rs:852-863).
  - Non-empty stdout means tracked; non-zero status is an error with git's trimmed stderr (lib.rs:864-869).
  - Tracked: kept, stderr info `Tracked by git, keeping: "<path>"` (lib.rs:932) [source; tests/test_build_artifacts.rs:237]. Not observed: repo rules forbid `git add` in experiments.
  - Error (incl. git missing, canonicalize failure): kept, warning `Cannot read "<path>": git cannot say whether it is tracked: <err>`, exit 3 (lib.rs:935-938) [observed with `PATH=/nonexistent`: `... No such file or directory (os error 2)`].
  - A kept (tracked or unclassified) artifact dir is then descended into like any directory (lib.rs:1063) [source].
- Attribution: pattern string `build-artifact`, reason `build-artifact` (lib.rs:1107-1111) [observed].

## 8. Dependencies (-D)

| Dir | Lock file beside it | Restore | Cite |
|-|-|-|-|
| `.venv` | `uv.lock` | `uv sync` | constants.rs:124 |
| `node_modules` | `package-lock.json` | `npm ci` | constants.rs:125 |
| `node_modules` | `npm-shrinkwrap.json` | `npm ci` | constants.rs:126 |
| `node_modules` | `yarn.lock` | `yarn install --immutable` | constants.rs:127 |
| `node_modules` | `pnpm-lock.yaml` | `pnpm install --frozen-lockfile` | constants.rs:128-132 |
| `node_modules` | `bun.lock` | `bun install --frozen-lockfile` | constants.rs:134 |
| `node_modules` | `bun.lockb` | `bun install --frozen-lockfile` | constants.rs:135 |
| `vendor` | `go.mod` | `go mod vendor` | constants.rs:136 |

- Rule (lib.rs:874-884, 1033-1040): real directory, basename in table, lock is a regular file by lstat (symlinked lock does not count). First table row that matches gives the restore command.
- No `.git` or outermost-project requirement [source].
- Checked after protected and configured excludes, before built-in excludes and patterns (lib.rs:1031-1044). So `.venv` + `uv.lock` matches despite `**/.venv` [observed]; a `-g '**/node_modules'` match beside a lock reports `dependency` [source].
- Git-tracked guard as in section 7; tracked tree falls through to the remaining checks (built-in exclude, patterns) and may be descended (lib.rs:1035-1044) [source].
- Matched tree is not entered (lib.rs:1037).
- Text line adds `  restore: <cmd>`; JSON adds `restore` (lib.rs:1521-1537, 521-522) [observed].
- Customisation: none; table is compile-time (constants.rs:123).

## 9. Filters

- `--older-than` / `older_than_secs` (lib.rs:1294-1310, 632-636, 654-753):
  - Grammar: trim whitespace; last char is unit `s`=1, `m`=60, `h`=3600, `d`=86400, `w`=604800 (lowercase only); prefix is ASCII digits only, non-empty, fits u64; product must not overflow (lib.rs:91-139). `' 7d '` ok, `0s` ok, `1.5h`, `+5s`, `5S`, `5` rejected [observed].
  - Timestamp: mtime (`modified()`) of the lstat metadata. Unreadable mtime counts as recent (kept) (lib.rs:634-636).
  - Cutoff = now - secs. Recent means `mtime > cutoff` (strict). Future timestamps are recent [observed `future.pyc` kept].
  - If now - secs underflows, every target is dropped (lib.rs:1296-1309) [observed `-o 9999999999w`: no output, exit 0].
  - File/symlink target: own mtime; symlink aged by link's own timestamp [observed `old/l.pyc`].
  - Directory target: kept if its own mtime is recent, or any entry at any depth (files, symlinks, subdirectories) is recent, or any listing/stat inside fails (lib.rs:686-742, 1307-1309) [observed: `new/__pycache__` (old dir, fresh file) kept].
- `--larger-than` / `larger_than_bytes` (lib.rs:146-181, 1311-1313):
  - Grammar: trim; number = digits with optional single `.` (`5.`, `.5` ok; `.` alone error); suffix exactly one of `""`, `B`, `K`, `KiB`, `M`, `MiB`, `G`, `GiB`, `T`, `TiB` (case-sensitive, binary: 2^10..2^40). Fraction up to 26 digits; result = whole*mult + floor(fraction*mult/10^len); must fit u64 [observed: `1.5K` ok, `1.5k`, `1KB`, `1e3`, `+1`, `1G2`, `16384P`, 28-digit fraction rejected; `99999999T` too large].
  - Keeps targets with `size >= limit` (inclusive). Directory size = total from section 10.
- Combination: AND. Age filter first, then size filter, both after sizing and before the report, so totals and listing reflect filtered targets (lib.rs:1302-1314).

## 10. Sizing

- Apparent bytes (`Metadata::len`), not allocated blocks (lib.rs:718, 1101).
- File target: lstat length. Symlink target: link length. Fifo: 0 [observed].
- Directory target: sum of lstat length of every non-directory entry at any depth; directory entries contribute 0; symlinks counted at own length, never followed (lib.rs:647-753) [observed: dir with 3000-byte file, hard link to it, symlink `h1` = `5.86 KiB` = 6002 B].
- Hard links: no dedup, each link counted [observed].
- Entry count per directory (all entries, recursively) feeds the removal progress bar (lib.rs:699, 1577).
- `format_size` (lib.rs:212-230):
  - `< 1024`: `"{n} B"` integer.
  - else largest of KiB/MiB/GiB/TiB with value >= 1 unit: `"{:.2} KiB"` etc., value `bytes as f64 / 1024^k`.
  - Rounding: Rust `{:.2}` on the exact binary value, ties to even (0.125 -> `0.12`, 0.375 -> `0.38`) [observed in scratch rustc test].
  - 1048575 B -> `1024.00 KiB` (threshold is >= 1 MiB, not rounded value) [observed].
  - No unit above TiB.
  - Examples: `0 B`, `1023 B`, `1.00 KiB`, `1.50 KiB`, `1.00 MiB`, `1.00 TiB` (tests/test_format_size.rs).

## 11. Confirmation

- Asked only when targets non-empty, not `-y`, not dry run (lib.rs:1321-1330). JSON mode also asks [observed].
- Prompt text: `Do you want to delete the above?` (lib.rs:1325).
- stdin and stderr both terminals (lib.rs:1760-1771):
  - Unix: `tcflush(STDIN, TCIFLUSH)` discards typeahead first (lib.rs:1762-1766).
  - dialoguer `Confirm` without default, on stderr: shows `Do you want to delete the above? [y/n] `.
  - Accepted keys: `y`/`Y` -> yes, `n`/`N` -> no. Other keys and Enter are ignored; prompt waits [observed: `x`, `\r` ignored, then `Y` accepted].
  - After answer, line rewritten to `Do you want to delete the above? yes` or `... no` [observed].
  - Interaction error: `IO error: ...`, exit 1 (lib.rs:1770).
- Otherwise (non-tty stdin or stderr) (lib.rs:1773-1778):
  - Prints `Do you want to delete the above? [y/N] ` to stderr, no newline.
  - Reads exactly 1 byte; proceed only if `y` or `Y`. `Yes` proceeds, `' y'` cancels, `n` cancels, EOF cancels, read error cancels [observed].
- Cancel: `warning: Cleaning operation cancelled.` on stderr, JSON `cancelled: true`, exit 0 unless scan warnings (exit 3) (lib.rs:1327-1329; main.rs:237-244) [observed].
- `-q` does not suppress the prompt, but suppresses the listing (see section 19) [observed].
- Dry run: never prompts (lib.rs:1321-1324).

## 12. Removal

- Skipped entirely in dry run (lib.rs:1332-1334).
- Parallel: `min(available_parallelism, targets)` threads pull indices from an atomic counter; results stored per index and reported in sorted list order after all threads finish (lib.rs:1591-1635).
- Pre-check (all platforms): scanned type must be dir, file or symlink, else `unsupported file type` (lib.rs:1711-1713) [observed for fifo].
- Unix algorithm (remove.rs:27-75):
  - rel = target path minus walk root; `..`/root/prefix components -> `is not below the working directory`; empty -> `is the working directory itself`; NUL -> `"<name>" contains a NUL byte` (remove.rs:33-43, 134-136).
  - Open canonical root with `O_RDONLY|O_DIRECTORY|O_CLOEXEC` (follows) (remove.rs:45-46); each parent component via `openat` with `O_NOFOLLOW` (remove.rs:47-50). Errors: `cannot open "<root>": <err>`, `cannot open component "<name>": <err>`.
  - Directory target: `openat` O_NOFOLLOW, `fstat`, compare dev, ino, `S_IFMT` with scan (remove.rs:53-60, 198-203); mismatch -> `replaced since it was scanned`. Then empty recursively through descriptors: names read fully first, `DT_UNKNOWN` resolved by `fstatat(AT_SYMLINK_NOFOLLOW)`, subdirs opened O_NOFOLLOW, `unlinkat` per entry, then `unlinkat(AT_REMOVEDIR)` the target (remove.rs:61-63, 79-99). Inner entries are not identity-checked.
  - Non-directory: `fstatat` NOFOLLOW compare then `unlinkat(0)` (remove.rs:64-72).
  - Symlinks inside a target are unlinked, not followed (lib.rs:1892-1916).
- Non-Unix (lib.rs:1678-1736): canonicalize parent, require `resolved/name` under base (`now resolves outside the working directory`, `cannot resolve parent: <err>`), re-stat and compare (`replaced since it was scanned`, `cannot re-check: <err>`), then `remove_dir_all` or `remove_file` by path.
  - Windows identity: file type + volume serial + 128-bit file ID via `GetFileInformationByHandleEx(FileIdInfo)`, opened with `FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT` (file_id.rs:23-46; lib.rs:776-783).
  - Other non-Unix, non-Windows: type only (lib.rs:781-782).
- Read-only directories: no chmod. A target containing a 0555 subdir fails with `Permission denied (os error 13)` and is left partly emptied [observed: `t/build/rodir/f` survived; error names `t/build`]. A file in a 0555 parent fails likewise [observed].
- First error aborts that target (`?` propagation, remove.rs:82-97); partial contents may already be gone [observed: `ro/__pycache__` emptied, then its own unlink failed].
- Failure line: `error: Failed to remove "<path>": <reason>` on stderr per failed target, then `error: N removal(s) failed` (lib.rs:1630, 1646-1648) [observed]. Exit 1.
- `removed_count` = targets fully removed; `removed_size` = sum of their scan-time sizes; failed targets add nothing even if partly deleted (lib.rs:1619-1621).

## 13. Text output

Streams: lines logged under target `reclean::report` go to stdout; all other log records and the prompt go to stderr (main.rs:131-160; lib.rs:627). `-l`, `--completions`, JSON use `println!` to stdout (main.rs:234, 340, 353-355).

stdout, in order (lib.rs:1480-1562, 1659-1669, 1617-1645) [observed]:

```

Matched targets:
  <path>[/]  <size>[  restore: <cmd>]
  ...

<N> target|targets, <size> to reclaim
  <reason:<16><count:>6>  <size:>12>     (only if >1 distinct reason; order pattern, broken-symlink, build-artifact, dependency)

=== Statistics ===                         (only -s, text mode)
  <pattern>: <count> item(s), <size>
==================
<empty line>
  removed  <path>[/]                       (only -v, per removed target, list order)
Deleted <n> item(s) totalling <size>       (only if removed_count > 0)
```

- Header line is preceded by an empty line (`"\nMatched targets:"`). Nothing at all is printed when no targets match [observed].
- Singular `1 target`, else `targets` (lib.rs:1547).
- Reason row example: `  pattern              1           0 B`, `  broken-symlink       2          15 B` [observed].
- Stats order: count descending, then pattern ascending (lib.rs:1652-1656). Non-pattern targets appear as `build-artifact`, `dependency`, `broken-symlink` [observed].
- Paths: target path with `config.path` prefix stripped (component-wise); directories (by lstat) get a trailing `/` (lib.rs:1467-1477).
- stderr lines [observed unless noted]:
  - `warning: Cannot read "<path>": <err>`
  - `warning: N path(s) could not be read; matches beneath them were not considered`
  - `warning: Cleaning operation cancelled.`
  - `error: Failed to remove "<path>": <reason>`, `error: N removal(s) failed`
  - `using settings file: "<abs path>"` (info, no prefix) (main.rs:275)
  - `generating default '.reclean.toml' file` (main.rs:211)
  - `Tracked by git, keeping: "<path>"` [source lib.rs:932]
  - `debug: Excluded: "<path>"`, `debug: Protected, skipping: "<path>"` (only -v)
  - Prefixes: info none, `error: `, `warning: `, `debug: `, `trace: ` (main.rs:141-147).
- Colour (main.rs:165-175, 431-436):
  - `--color auto`: per stream, on if that stream is a terminal and `NO_COLOR` unset or empty. Empty `NO_COLOR` keeps colour [observed].
  - `always` ignores `NO_COLOR` and pipes [observed]; `never` off.
  - stdout and stderr decided separately: stdout redirected to file is plain while stderr prompt/warnings stay coloured [observed].
  - `TERM` is not consulted for report/log colour: `TERM=dumb` under a pty still coloured [observed]. Clap's own error output was uncoloured under `TERM=dumb` (anstream behaviour) [observed]; spinner hidden [observed].
  - Clap help/errors follow `--color` via a pre-scan, last valid value before `--` wins (main.rs:177-194, 418-420).
  - SGR codes [observed]: directory path cyan `36`; size and restore suffixes dim `2`; count and total size bold `1`; reason rows dim; `Deleted N item(s)` green `32` only if no failures (lib.rs:1638-1643); `removed` dim; `error: ` `31`; `warning: ` `33`; `debug: ` `2`. Only the prefix is coloured in log lines.
- Progress (stderr, indicatif; hidden when stderr is not a terminal; off with `-q`) (lib.rs:1273-1282, 1575-1590):
  - Scan: template `  {msg}`, message `scanning`, then `scanning N` every 256 entries, `sizing N` every 64 directories; cleared on finish [observed `  scanning` line then `\r\x1b[2K`].
  - Removal: `  removing [{bar:30}] {percent:>3}%  {pos} / {len} entries`, chars `#-`; total = sum(entries+1) per target; cleared on finish [observed `  removing [##############################] 100%  5 / 5 entries`]. Windows advances per whole target (lib.rs:1729-1734).
  - Also shown in JSON mode (not gated on format) [source lib.rs:1275].
- `-v`: log level Debug for `reclean*` targets only (main.rs:132-135); adds debug lines and `removed` lines.
- `-q`: log level Error. Suppresses report, info, warnings; keeps errors, prompt, JSON, `-l`, completions [observed].
- Escaping (report paths only, lib.rs:1742-1752): non-UTF-8 lossily converted (U+FFFD) [observed `bad\xff.pyc` -> `bad?.pyc` as U+FFFD]; `char::is_control` chars via `escape_default` (`\u{1}`, `\n`, `\t`, `\r`) [observed `ctl\u{1}.pyc`]; `\` -> `\\` except on Windows [observed `we\\ird.pyc`]. Warning/error lines instead use Rust `{:?}` quoting of the display string (lib.rs:640, 1630).

## 14. JSON

- `--format json`: report text suppressed; one pretty-printed (2-space) JSON document on stdout after the run (main.rs:230-235; lib.rs:1183-1252). stderr still gets warnings, errors, prompt.
- Keys are emitted in alphabetical order (serde_json `Value` map without preserve_order) [observed].
- Schema:
  - `schema`: int, always `1`.
  - `config`: string path of config file used, or `null` (lib.rs:1234).
  - `matches`: array of objects (lib.rs:505-523, 1500-1515):
    - `path`: string, walk path including root text, not stripped (`./a/b.pyc`, `proj/build`, absolute when `-p` absolute), lossy UTF-8, JSON escaping.
    - `size`: int bytes.
    - `pattern`: string; matching pattern, or reason name for non-pattern targets.
    - `reason`: `pattern` | `broken-symlink` | `build-artifact` | `dependency`.
    - `type`: `directory` | `file` | `symlink` (by lstat; fifo reports `file`).
    - `restore`: string, present only for dependency targets.
  - `summary`: object: `total_count` int, `total_size` int, `total_size_human` string (format_size), `dry_run` bool, `cancelled` bool, `removed_count` int, `removed_size` int.
  - `stats`: array of `{pattern, count, size, size_human}`, sorted count desc then pattern; empty unless `-s` (lib.rs:1494-1498).
  - `reasons`: array of `{reason, count, size}` in enum order; always populated.
  - `failures`: array of `{path, error}`.
  - `warnings`: array of `{path, error}`.
- Dry run: `dry_run: true`, `removed_*: 0`. Cancelled: `cancelled: true`, `removed_*: 0` [observed]. No match: empty arrays, zeros.
- Removal failures: listed in `failures` with error text, e.g. `"Permission denied (os error 13)"` [observed]; exit 1.
- JSON is not printed when `run()` returns an error (bad root, bad glob) (main.rs:228).
- Real example (`reclean -d -B -D -s -p proj --format json`, exit 3; stderr `warning: Cannot read "proj/locked": Permission denied (os error 13)`) [observed]:

```json
{
  "config": null,
  "failures": [],
  "matches": [
    {
      "path": "proj/.venv",
      "pattern": "dependency",
      "reason": "dependency",
      "restore": "uv sync",
      "size": 0,
      "type": "directory"
    },
    {
      "path": "proj/__pycache__",
      "pattern": "**/__pycache__",
      "reason": "pattern",
      "size": 4,
      "type": "directory"
    },
    {
      "path": "proj/build",
      "pattern": "build-artifact",
      "reason": "build-artifact",
      "size": 0,
      "type": "directory"
    },
    {
      "path": "proj/dist",
      "pattern": "build-artifact",
      "reason": "build-artifact",
      "size": 0,
      "type": "directory"
    },
    {
      "path": "proj/node_modules",
      "pattern": "dependency",
      "reason": "dependency",
      "restore": "npm ci",
      "size": 0,
      "type": "directory"
    },
    {
      "path": "proj/stale.pyc",
      "pattern": "**/*.pyc",
      "reason": "pattern",
      "size": 4,
      "type": "file"
    },
    {
      "path": "proj/target",
      "pattern": "build-artifact",
      "reason": "build-artifact",
      "size": 0,
      "type": "directory"
    },
    {
      "path": "proj/vendor",
      "pattern": "dependency",
      "reason": "dependency",
      "restore": "go mod vendor",
      "size": 0,
      "type": "directory"
    }
  ],
  "reasons": [
    { "count": 2, "reason": "pattern", "size": 8 },
    { "count": 3, "reason": "build-artifact", "size": 0 },
    { "count": 3, "reason": "dependency", "size": 0 }
  ],
  "schema": 1,
  "stats": [
    { "count": 3, "pattern": "build-artifact", "size": 0, "size_human": "0 B" },
    { "count": 3, "pattern": "dependency", "size": 0, "size_human": "0 B" },
    { "count": 1, "pattern": "**/*.pyc", "size": 4, "size_human": "4 B" },
    { "count": 1, "pattern": "**/__pycache__", "size": 4, "size_human": "4 B" }
  ],
  "summary": {
    "cancelled": false,
    "dry_run": true,
    "removed_count": 0,
    "removed_size": 0,
    "total_count": 8,
    "total_size": 8,
    "total_size_human": "8 B"
  },
  "warnings": [
    { "error": "Permission denied (os error 13)", "path": "proj/locked" }
  ]
}
```

(Inner one-line objects compacted here for length; the program prints each key on its own line.)

## 15. Exit codes

| Code | Triggers | Cite |
|-|-|-|
| 0 | success; nothing matched; dry run; cancelled prompt (no warnings); root is a file; `-l`; `--completions`; `-w` success; `-h`/`-V` | main.rs:237-244, 341, 358, 409 |
| 1 | `CleanError::IoError`: root canonicalize failure, root lstat failure, tty prompt interaction error, `-w` write I/O error; any removal failure (takes precedence over 3) | main.rs:237-238, 448-451; lib.rs:1263, 1405, 1770 |
| 2 | clap parse errors; `ConfigError` (bad duration/size, config not found/unreadable/invalid, `-w` target exists, JSON/TOML serialize failure); `GlobError` (bad pattern) | main.rs:447-450 |
| 3 | run completed with at least one warning (unreadable path, git unable to classify, Windows file-id failure at collect) and no removal failure | main.rs:221-242 |
| 101 | Rust panic, e.g. `println!` to a closed stdout in `-l`, `--completions`, JSON [observed for `-l | head -1`: `failed printing to stdout: Broken pipe (os error 32)`] | main.rs:234, 340, 353-355 |

## 16. Config file

- Name: `.reclean.toml` (constants.rs:4).
- Used only with `-c` (main.rs:361-363). Bare `-c` or `-c ""`: discover; `-c PATH`: explicit, relative to cwd.
- Explicit path: `Path::exists` check, else `Configuration error: Settings file 'missing.toml' not found` exit 2 (main.rs:251-260) [observed]. A directory passes the check and fails at read: `Configuration error: Cannot read "<path>": Is a directory (os error 21)` [observed].
- Discovery (lib.rs:232-302):
  1. From cwd (not `--path`), each directory up to filesystem root: first `.reclean.toml` that `is_file` (follows symlinks; directories skipped) (lib.rs:234-245). No repository or home boundary.
  2. Global, first existing file of:
     - Linux/other Unix: `$XDG_CONFIG_HOME/reclean/config.toml` if `XDG_CONFIG_HOME` has a root, else `~/.config/reclean/config.toml` (lib.rs:286-296). Relative XDG ignored [observed].
     - macOS: same, then `~/Library/Application Support/reclean/config.toml` with warning `reading "<old>", the 0.5.0 location; move it to "<new>"` (lib.rs:252-257, 294).
     - Windows: `%APPDATA%\reclean\config.toml` only (`dirs::config_dir`) (lib.rs:293).
  3. None: `Configuration error: No '.reclean.toml' found in directory tree or global config (["<loc1>", ...])`, exit 2 [observed].
- On load: stderr `using settings file: "<path>"` (main.rs:275) [observed].
- Keys (lib.rs:312-357, 359-380); `#[serde(default, deny_unknown_fields)]`:

| Key | Type | Default if omitted | Meaning |
|-|-|-|-|
| `path` | string | `"."` | root |
| `patterns` | array of string | built-in list | include patterns; `[]` matches nothing |
| `exclude_patterns` | array of string | built-in excludes | replaces built-ins |
| `dry_run` | bool | false | |
| `skip_confirmation` | bool | false | |
| `include_symlinks` | bool | false | |
| `remove_broken_symlinks` | bool | false | |
| `stats_mode` | bool | false | |
| `older_than_secs` | u64 integer seconds | none | age filter (no duration string) |
| `show_progress` | bool | false | accepted, then overwritten by CLI with `!quiet` (main.rs:299) |
| `protected_dirs` | array of string | built-in list | `[]` disables |
| `build_artifacts` | bool | false | |
| `dependencies` | bool | false | |
| `larger_than_bytes` | u64 integer | none | size filter |

- `json_mode` and `config_file` are not keys (serde skip); `json_mode = true` is an unknown-field error [observed].
- Unknown key error [observed]:
  `error: Configuration error: Cannot deserialize from .toml: TOML parse error at line 2, column 1` + caret snippet + ``unknown field `bogus`, expected one of `path`, `patterns`, `exclude_patterns`, `dry_run`, `skip_confirmation`, `include_symlinks`, `remove_broken_symlinks`, `stats_mode`, `older_than_secs`, `show_progress`, `protected_dirs`, `build_artifacts`, `dependencies`, `larger_than_bytes` ``, exit 2.
- Wrong type: `invalid type: string "yes", expected a boolean`, exit 2 [observed].
- Precedence (main.rs:283-330): config, then CLI:
  - Flags `-d -y -i -r -s -B -D` can only set true.
  - `-p`, `-g` replace; `-e` appends; `-o`, `--larger-than` replace.
  - `--no-protect` clears `protected_dirs` and drops built-in-equal excludes.
  - `show_progress` always `!-q`; `json_mode` from `--format`.
- TOML dialect: full TOML via `toml` 0.9 + serde; no custom subset [source Cargo.toml; main.rs:279].
- `-w` (main.rs:203-219, 386-410): serializes the CLI-built config with `toml::to_string` to `./.reclean.toml` (cwd, not `--path`); refuses if it exists (`Configuration error: Cannot overwrite existing '.reclean.toml' file`, exit 2) [observed]. CLI options are reflected; `None` options omitted. Default output verbatim [observed]:

```toml
path = "."
patterns = ["**/.DS_Store", "**/Thumbs.db", "**/__pycache__", "**/.coverage", "**/.mypy_cache", "**/.pylint_cache", "**/.pytest_cache", "**/.ruff_cache", "**/.rumdl_cache", "**/.pyscn", "**/.ropeproject", "**/pip-log.txt", "**/*.pyc", "**/*.pyo"]
exclude_patterns = ["**/.venv", "**/venv"]
dry_run = false
skip_confirmation = false
include_symlinks = false
remove_broken_symlinks = false
stats_mode = false
show_progress = true
protected_dirs = [".git", ".hg", ".svn", ".config", ".ssh", ".gnupg"]
build_artifacts = false
dependencies = false
```

  With `-g '*.x' -e foo -o 1h --larger-than 1K -d -s -B --no-protect -p sub`: `path = "sub"`, `patterns = ["*.x"]`, `exclude_patterns = ["foo"]`, `older_than_secs = 3600` (after `stats_mode`), `protected_dirs = []`, `larger_than_bytes = 1024` (last) [observed].

## 17. Platform differences

- Removal: Unix descriptor-relative with dev/ino/type check (remove.rs); Windows path-based with volume serial + file ID + type (file_id.rs; lib.rs:1678-1736); other targets type-only (lib.rs:781).
- Windows: `Scanned::of` calls `file_id`; failure skips the target with a warning (lib.rs:764-770, 1091-1096) [source].
- Windows: removal progress advances per target (lib.rs:1729-1734).
- Windows: backslash not escaped in report (lib.rs:1746).
- Typeahead flush (`tcflush`) Unix only (lib.rs:1762).
- Global config locations differ (section 16).
- Glob backslash semantics differ per globset platform default [inference].
- Unix: `DT_UNKNOWN` handled by `fstatat` (remove.rs:83-87), relevant on filesystems without `d_type`.

## 18. Other

- `-l` output on stdout, five sections, heading `<name>:` then `  <item>` lines (main.rs:344-358) [observed]: `default patterns:`, `protected directories:`, `default excludes:`, `build artifact directories:` (distinct, sorted), `dependency directories:` (distinct, sorted). Survives `-q`.
- `--completions <shell>`: clap_complete script for `reclean` on stdout (main.rs:338-342) [observed bash, zsh].
- Library API (lib.rs, constants.rs), all `pub`:
  - `constants`: `SETTINGS_FILENAME`, `PROTECTED_DIRS`, `DEFAULT_EXCLUDES`, `BUILD_ARTIFACTS`, `DEFAULT_PATTERNS`, `DEPENDENCIES`, and `get_*` owned-copy functions.
  - `CleanError {IoError, GlobError, ConfigError}`, `Result<T>`.
  - `parse_duration`, `parse_size`, `format_size`, `find_config_upward`, `global_config_path`, `global_config_locations`, `discover_config`.
  - `Reason` (+`name()`), `MatchedItem`, `REPORT_TARGET = "reclean::report"`.
  - `CleanConfig` (+`Default`, `builder()`), `CleanConfigBuilder` (one setter per field except `config_file`).
  - `CleaningJob::new`, `run`, `to_json`, `has_failures`, `has_warnings`; pub fields `config`, `size`, `counter`, `stats`, `reasons`, `cancelled`, `removed_count`, `removed_size`, `failed_deletions`, `matched_items`, `warnings`.
  - `run()` resets prior results (lib.rs:1256). Library prints nothing directly except the prompt; output goes through `log` (CHANGELOG.md:31).
- `CleanConfig::default()` has `patterns: []`, unlike serde default (lib.rs:363 vs 320).
- Multiple `-g` values compile into one GlobSet; per-pattern matchers kept for attribution (lib.rs:1351-1375).

## 19. Known gaps, bugs, README disagreements

Gaps/bugs (marked as such; inference where noted):

- [BUG, observed] `-q` without `-y` suppresses the listing yet still prompts `Do you want to delete the above?`, so the user confirms blind (main.rs:424-426; lib.rs:1322-1325).
- [BUG, observed] `println!` in `-l`, `--completions` and JSON output panics on a closed pipe, exit 101 (main.rs:234, 340, 353-355). Report lines ignore the error instead (main.rs:148-152).
- [GAP, source] A git-tracked or unclassifiable `-B`/`-D` candidate is descended into, so patterns can still match inside it (lib.rs:1035-1063).
- [GAP, source] `-B` marker test follows symlinks (`is_file`) and `.git` beside uses `exists()` (follows; a dangling `.git` symlink counts as absent), while the outermost test and `-D` lock test use lstat (lib.rs:811, 817, 830, 881).
- [GAP, observed] Removal failure leaves a partly emptied directory; no chmod retry; `removed_size` excludes the deleted part; the error names the target, not the failing inner entry.
- [GAP, observed] Hard links counted once per link; sizes are apparent, not allocated.
- [GAP, observed] Text paths are root-relative; JSON paths keep the root prefix (`./`, `proj/`, absolute).
- [GAP, observed] Warning/error lines quote paths with Rust `{:?}` rather than `escaped()`; Unicode format chars such as U+202E are not `is_control` and pass unescaped in the listing [source for U+202E].
- [GAP, source] `Invalid size '<x>'` echoes the untrimmed input; `too large` echoes the trimmed one (lib.rs:147-148, 174).
- [GAP, source] fifos, sockets, devices matched by a pattern are listed (JSON `type: "file"`) and then always fail with `unsupported file type`, exit 1 [observed for fifo].
- [GAP, source] No filesystem-boundary option; mounts are crossed.
- [GAP, source] `-c PATH` existence check accepts a directory; error comes later at read.
- [GAP, source] In `-c` mode, boolean config values set to true cannot be turned off from the CLI.
- [NOTE] `-P` is a hidden no-op (main.rs:82-84).
- [NOTE] Scan-phase warnings sort nondeterministically.

README vs source:

- README.md:302 "`y` proceeds; anything else cancels": on a terminal only `y`/`Y`/`n`/`N` answer; other keys and Enter are ignored, and the prompt reads `[y/n]` [observed]. The `[y/N]` single-byte rule applies only to non-tty stdin.
- README.md:224 "The document opens with `"schema": 1`": keys print alphabetically; `schema` is not first [observed].
- README.md:317 exit 1 for "confirmation could not be read": true only for the tty path; a piped read error cancels with exit 0 (lib.rs:1776-1778).
- README.md:381 says non-Unix identity is "compared by file type", "so a file replaced by another of the same type is not caught": [Unreleased] Windows also compares volume serial and file ID (lib.rs:776-783; CHANGELOG.md:43). True only for non-Unix, non-Windows targets.
- README.md:17 "Metadata caching": no cache found in source [inference from reading lib.rs].
- Help text `-c ... (searches upward, then ~/.config/reclean/)` (main.rs:38): omits `$XDG_CONFIG_HOME`, the macOS fallback and Windows `%APPDATA%`.
- README.md:147 "every flag `false`": `show_progress` is written as `true` by `-w` and ignored by the CLI either way (main.rs:299).
- README.md:296 "lowercase suffixes are errors": correct, but `KB`, `MB` (decimal style) are also errors [observed]; not stated.
