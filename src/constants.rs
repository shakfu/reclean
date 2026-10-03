// --------------------------------------------------------------------
// constants

pub const SETTINGS_FILENAME: &str = ".reclean.toml";

/// Directories never matched and never entered.
///
/// These hold data whose loss is expensive and unrecoverable -- repository
/// history, private keys, user configuration -- while their contents also match
/// ordinary cleaning patterns: a git object store holds files named like build
/// artifacts. Protection is by name and applies to any entry type, so the `.git`
/// *file* that marks a submodule is covered too. Virtualenv directories are not
/// on the list: they are rebuilt from a lockfile, and cleaning the
/// `__pycache__` trees inside one is a thing users ask for.
pub const PROTECTED_DIRS: &[&str] = &[".git", ".hg", ".svn", ".config", ".ssh", ".gnupg"];

/// The default protected directory names, owned.
pub fn get_protected_dirs() -> Vec<String> {
    PROTECTED_DIRS.iter().map(|s| s.to_string()).collect()
}

/// Glob patterns excluded from the walk by default.
///
/// A virtualenv is rebuilt from a lockfile, so its contents may not be worth the
/// scan: an installed environment holds `__pycache__` directories by the
/// hundred, all of which the default patterns match. This is an ordinary
/// exclude, not protection: `--no-protect` drops it, `exclude_patterns` in a
/// config file replaces it, and naming a virtualenv on `--path` cleans it.
pub const DEFAULT_EXCLUDES: &[&str] = &["**/.venv", "**/venv"];

/// The default exclude patterns, owned.
pub fn get_default_excludes() -> Vec<String> {
    DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect()
}

/// Build output directories that `--build-artifacts` may remove, each paired
/// with a file that marks a project generating it.
///
/// `build` and `target` are ordinary names, so a directory qualifies only when
/// the paired marker *and* `.git` both sit beside it. Requiring `.git` pins the
/// match to the top level of a project: a CMake subdirectory carries its own
/// `CMakeLists.txt`, so the marker alone would claim `src/program/build` as
/// well. Several ecosystems build into the same directory name, so one name
/// carries several markers.
pub const BUILD_ARTIFACTS: &[(&str, &str)] = &[
    // C and C++
    ("build", "CMakeLists.txt"),
    ("build", "meson.build"),
    // Rust
    ("target", "Cargo.toml"),
    // JavaScript and TypeScript
    ("build", "package.json"),
    ("dist", "package.json"),
    (".next", "package.json"),
    (".nuxt", "package.json"),
    (".svelte-kit", "package.json"),
    (".turbo", "package.json"),
    (".parcel-cache", "package.json"),
    // JVM
    ("target", "pom.xml"),
    ("build", "build.gradle"),
    ("build", "build.gradle.kts"),
    (".gradle", "build.gradle"),
    (".gradle", "build.gradle.kts"),
    // Python
    ("build", "pyproject.toml"),
    ("dist", "pyproject.toml"),
    ("build", "setup.py"),
    ("dist", "setup.py"),
    // Zig
    ("zig-out", "build.zig"),
    ("zig-cache", "build.zig"),
    (".zig-cache", "build.zig"),
    // Swift
    (".build", "Package.swift"),
    // Elixir
    ("_build", "mix.exs"),
    // Dart and Flutter
    ("build", "pubspec.yaml"),
];

/// The distinct artifact directory names, sorted.
pub fn get_artifact_dirs() -> Vec<String> {
    let mut names: Vec<String> = BUILD_ARTIFACTS.iter().map(|(d, _)| d.to_string()).collect();
    names.sort();
    names.dedup();
    names
}

/// Glob patterns matched when none are given.
///
/// Caches and editor debris only: each is regenerated without the network.
/// Build output and dependency trees have ordinary names, so they are matched by
/// project layout instead, under `--build-artifacts` and `--dependencies`.
pub const DEFAULT_PATTERNS: &[&str] = &[
    "**/.DS_Store",
    "**/Thumbs.db",
    "**/__pycache__",
    "**/.coverage",
    "**/.mypy_cache",
    "**/.pylint_cache",
    "**/.pytest_cache",
    "**/.ruff_cache",
    "**/.rumdl_cache",
    "**/.pyscn",
    "**/.ropeproject",
    "**/pip-log.txt",
    "**/*.pyc",
    "**/*.pyo",
];

/// The default patterns, owned.
pub fn get_default_patterns() -> Vec<String> {
    DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect()
}

/// Dependency trees that `--dependencies` may remove: the directory, the lock
/// file that must sit beside it, and the command that restores it.
///
/// The marker is the file that pins versions, not the one that declares them:
/// only a lock names the exact tree the restore command puts back. A directory
/// with no lock beside it is left alone.
pub const DEPENDENCIES: &[(&str, &str, &str)] = &[
    (".venv", "uv.lock", "uv sync"),
    ("node_modules", "package-lock.json", "npm ci"),
    ("node_modules", "npm-shrinkwrap.json", "npm ci"),
    ("node_modules", "yarn.lock", "yarn install --immutable"),
    (
        "node_modules",
        "pnpm-lock.yaml",
        "pnpm install --frozen-lockfile",
    ),
    // bun wrote the binary `bun.lockb` before 1.2, and `bun.lock` since
    ("node_modules", "bun.lock", "bun install --frozen-lockfile"),
    ("node_modules", "bun.lockb", "bun install --frozen-lockfile"),
    ("vendor", "go.mod", "go mod vendor"),
];

/// The distinct dependency directory names, sorted.
pub fn get_dependency_dirs() -> Vec<String> {
    let mut names: Vec<String> = DEPENDENCIES.iter().map(|(d, _, _)| d.to_string()).collect();
    names.sort();
    names.dedup();
    names
}
