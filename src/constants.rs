// --------------------------------------------------------------------
// constants

pub const SETTINGS_FILENAME: &str = ".rclean.toml";

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

/// Available preset names
pub const PRESET_NAMES: &[&str] = &["common", "python", "node", "rust", "java", "c", "go", "all"];

/// Get patterns for a named preset
pub fn get_preset_patterns(name: &str) -> Option<Vec<String>> {
    let patterns: Vec<&str> = match name {
        "common" => vec![
            "**/.DS_Store",
            "**/.bash_history",
            "**/.python_history",
            "**/Thumbs.db",
            "**/*.swp",
            "**/*.swo",
            // "**/*~",
        ],
        "python" => vec![
            "**/__pycache__",
            "**/.coverage",
            "**/.mypy_cache",
            "**/.pylint_cache",
            "**/.pytest_cache",
            "**/.ruff_cache",
            "**/.rumdl_cache",
            "**/.pyscn",
            "**/.ropeproject",
            "**/.python_history",
            "**/pip-log.txt",
            "**/*.pyc",
            "**/*.pyo",
            // "**/*.egg-info",
            // "**/dist",
        ],
        "node" => vec![
            "**/node_modules",
            "**/.next",
            "**/.nuxt",
            "**/.cache",
            "**/dist",
            "**/.parcel-cache",
            "**/.turbo",
            "**/.eslintcache",
            "**/coverage",
            "**/.nyc_output",
        ],
        "rust" => vec!["**/target"],
        "java" => vec![
            "**/*.class",
            "**/target",
            "**/.gradle",
            "**/build",
            "**/.settings",
            "**/.classpath",
            "**/.project",
        ],
        "c" => vec![
            "**/*.o",
            "**/*.obj",
            "**/*.a",
            "**/*.lib",
            "**/*.so",
            "**/*.dylib",
            "**/*.dll",
        ],
        "go" => vec!["**/vendor"],
        "all" => {
            let mut all = Vec::new();
            for preset in &["common", "python", "node", "rust", "java", "c", "go"] {
                if let Some(p) = get_preset_patterns(preset) {
                    for pattern in p {
                        if !all.contains(&pattern) {
                            all.push(pattern);
                        }
                    }
                }
            }
            return Some(all);
        }
        _ => return None,
    };

    Some(patterns.into_iter().map(String::from).collect())
}

/// Get the default patterns (python + common for backwards compatibility)
pub fn get_default_patterns() -> Vec<String> {
    let mut patterns = get_preset_patterns("common").unwrap_or_default();
    patterns.extend(get_preset_patterns("python").unwrap_or_default());

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    patterns.retain(|p| seen.insert(p.clone()));

    patterns
}
