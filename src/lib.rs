//! Safe, pattern-based file and directory cleaner.
//!
//! `reclean` recursively removes files and directories matching glob patterns,
//! with built-in safety measures including dry-run mode, confirmation prompts,
//! path traversal protection, and symlink guards.
//!
//! # Quick start
//!
//! ```no_run
//! use reclean::{CleanConfig, CleaningJob};
//!
//! let config = CleanConfig::builder()
//!     .path(".")
//!     .patterns(vec!["**/__pycache__".into(), "**/*.pyc".into()])
//!     .dry_run(true)
//!     .skip_confirmation(true)
//!     .build();
//!
//! let mut job = CleaningJob::new(config);
//! job.run().expect("cleaning failed");
//! ```

pub mod constants;

use dialoguer::Confirm;
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, error, info, warn};
use logging_timer::time;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

// --------------------------------------------------------------------
// error types

/// Error type for cleaning operations.
#[derive(Debug)]
pub enum CleanError {
    /// An I/O error occurred during file operations.
    IoError(std::io::Error),
    /// A glob pattern failed to compile.
    GlobError(globset::Error),
    /// A path resolved outside the allowed working directory.
    PathTraversal(PathBuf),
    /// Insufficient permissions to access or remove a path.
    PermissionDenied(PathBuf),
    /// A configuration or validation error.
    ConfigError(String),
}

impl std::fmt::Display for CleanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CleanError::IoError(e) => write!(f, "IO error: {}", e),
            CleanError::GlobError(e) => write!(f, "Glob pattern error: {}", e),
            CleanError::PathTraversal(p) => write!(f, "Path traversal detected: {:?}", p),
            CleanError::PermissionDenied(p) => write!(f, "Permission denied: {:?}", p),
            CleanError::ConfigError(s) => write!(f, "Configuration error: {}", s),
        }
    }
}

impl std::error::Error for CleanError {}

impl From<std::io::Error> for CleanError {
    fn from(err: std::io::Error) -> Self {
        CleanError::IoError(err)
    }
}

impl From<globset::Error> for CleanError {
    fn from(err: globset::Error) -> Self {
        CleanError::GlobError(err)
    }
}

/// Convenience alias for `std::result::Result<T, CleanError>`.
pub type Result<T> = std::result::Result<T, CleanError>;

// --------------------------------------------------------------------
// utilities

/// Parse duration string like "30d", "7d", "24h", "3600s" into seconds.
///
/// Supported units: s (seconds), m (minutes), h (hours), d (days), w (weeks).
pub fn parse_duration(duration: &str) -> Result<u64> {
    let duration = duration.trim();
    if duration.is_empty() {
        return Err(CleanError::ConfigError(
            "Duration cannot be empty".to_string(),
        ));
    }

    // The unit is one char, which need not be one byte
    let mut chars = duration.chars();
    let unit_part = chars.next_back().unwrap_or_default();
    let num_part = chars.as_str();
    if num_part.is_empty() {
        return Err(CleanError::ConfigError(format!(
            "Invalid duration '{}': must be a number followed by a unit (s, m, h, d, w)",
            duration
        )));
    }

    // `u64::from_str` accepts a leading `+`, which is not a duration
    if !num_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CleanError::ConfigError(format!(
            "Invalid number in duration: {}",
            num_part
        )));
    }
    let number: u64 = num_part.parse().map_err(|_| {
        CleanError::ConfigError(format!("Invalid number in duration: {}", num_part))
    })?;

    let multiplier: u64 = match unit_part {
        's' => 1,      // seconds
        'm' => 60,     // minutes
        'h' => 3600,   // hours
        'd' => 86400,  // days
        'w' => 604800, // weeks
        _ => {
            return Err(CleanError::ConfigError(format!(
                "Invalid duration unit '{}'. Use 's', 'm', 'h', 'd', or 'w'",
                unit_part
            )))
        }
    };

    // Wrapping would turn a huge age into a small one, and delete more
    number
        .checked_mul(multiplier)
        .ok_or_else(|| CleanError::ConfigError(format!("Duration '{}' is too large", duration)))
}

/// Parse a size like "100", "512B", "1.5K" or "2GiB" into bytes.
///
/// Suffixes are binary: K, M, G, T, or KiB, MiB, GiB, TiB. A fractional value
/// is converted exactly and truncated toward zero. Signs, exponents and anything
/// after the suffix are errors.
pub fn parse_size(size: &str) -> Result<u64> {
    let invalid = || CleanError::ConfigError(format!("Invalid size '{}'", size));
    let size = size.trim();
    let split = size
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(size.len());
    let (number, suffix) = size.split_at(split);
    let multiplier: u128 = match suffix {
        "" | "B" => 1,
        "K" | "KiB" => 1 << 10,
        "M" | "MiB" => 1 << 20,
        "G" | "GiB" => 1 << 30,
        "T" | "TiB" => 1 << 40,
        _ => return Err(invalid()),
    };

    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    // 26 digits keep `fraction * multiplier` inside u128
    if whole.is_empty() && fraction.is_empty() || fraction.contains('.') || fraction.len() > 26 {
        return Err(invalid());
    }
    let parse = |digits: &str| -> Result<u128> {
        if digits.is_empty() {
            Ok(0)
        } else {
            digits.parse().map_err(|_| invalid())
        }
    };
    let too_large = || CleanError::ConfigError(format!("Size '{}' is too large", size));
    let fractional = parse(fraction)? * multiplier / 10u128.pow(fraction.len() as u32);
    let bytes = parse(whole)?
        .checked_mul(multiplier)
        .and_then(|b| b.checked_add(fractional))
        .ok_or_else(too_large)?;
    u64::try_from(bytes).map_err(|_| too_large())
}

/// Why a target was matched, in order of what it costs to put back.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    /// A glob pattern: caches and debris, regenerated for free.
    Pattern,
    /// A symlink whose target is gone.
    BrokenSymlink,
    /// Build output, rebuilt offline. See [`constants::BUILD_ARTIFACTS`].
    BuildArtifact,
    /// A dependency tree, restored from the network. See [`constants::DEPENDENCIES`].
    Dependency,
}

impl Reason {
    /// The name used in output
    pub fn name(self) -> &'static str {
        match self {
            Reason::Pattern => "pattern",
            Reason::BrokenSymlink => "broken-symlink",
            Reason::BuildArtifact => "build-artifact",
            Reason::Dependency => "dependency",
        }
    }
}

/// Format a byte count as a human-readable string using binary units.
///
/// Uses IEC binary prefixes: B, KiB, MiB, GiB, TiB.
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    let size = bytes as f64;
    if size >= TIB {
        format!("{:.2} TiB", size / TIB)
    } else if size >= GIB {
        format!("{:.2} GiB", size / GIB)
    } else if size >= MIB {
        format!("{:.2} MiB", size / MIB)
    } else if size >= KIB {
        format!("{:.2} KiB", size / KIB)
    } else {
        format!("{} B", bytes)
    }
}

/// Search upward from `start_dir` for a file named `filename`.
/// Returns the path to the first match, or None.
pub fn find_config_upward(start_dir: &Path, filename: &str) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Return the path to the global config file, if it exists.
/// Checks `~/.config/reclean/config.toml`.
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("reclean").join("config.toml"))
        .filter(|p| p.is_file())
}

/// Discover a config file: first search upward for `.reclean.toml`, then fall back to global.
pub fn discover_config(start_dir: &Path) -> Option<PathBuf> {
    find_config_upward(start_dir, constants::SETTINGS_FILENAME).or_else(global_config_path)
}

// --------------------------------------------------------------------
// configuration

/// Serializable configuration for a cleaning job.
///
/// Construct via [`CleanConfig::builder()`] or deserialize from a `.reclean.toml` file.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct CleanConfig {
    /// Root directory to clean (default `"."`).
    pub path: String,
    /// Glob patterns to match for deletion.
    pub patterns: Vec<String>,
    /// Glob patterns to exclude. An excluded directory is not entered.
    /// Defaults to [`constants::DEFAULT_EXCLUDES`]; an empty list excludes nothing.
    #[serde(default = "constants::get_default_excludes")]
    pub exclude_patterns: Vec<String>,
    /// When `true`, report matches without deleting anything.
    pub dry_run: bool,
    /// When `true`, skip the interactive confirmation prompt.
    pub skip_confirmation: bool,
    /// When `true`, matched symlinks are eligible for removal.
    pub include_symlinks: bool,
    /// When `true`, broken symlinks are removed regardless of pattern matching.
    pub remove_broken_symlinks: bool,
    /// When `true`, display per-pattern match counts and sizes.
    #[serde(default)]
    pub stats_mode: bool,
    /// If set, only remove files whose last modification is older than this many seconds.
    #[serde(default)]
    pub older_than_secs: Option<u64>,
    /// When `true`, show a progress spinner during scanning.
    #[serde(default)]
    pub show_progress: bool,
    /// When `true`, produce JSON output instead of human-readable text. CLI-only, not serialized.
    #[serde(skip)]
    pub json_mode: bool,
    /// Directory names that are never matched and never entered. Defaults to
    /// [`constants::PROTECTED_DIRS`]; an empty list disables the protection.
    #[serde(default = "constants::get_protected_dirs")]
    pub protected_dirs: Vec<String>,
    /// When `true`, match build output at the top level of a project as well.
    /// See [`constants::BUILD_ARTIFACTS`].
    #[serde(default)]
    pub build_artifacts: bool,
    /// When `true`, match dependency trees beside their lock file as well.
    /// See [`constants::DEPENDENCIES`].
    #[serde(default)]
    pub dependencies: bool,
    /// If set, only remove targets of at least this many bytes.
    #[serde(default)]
    pub larger_than_bytes: Option<u64>,
    /// The configuration file this was read from, reported in JSON output.
    /// CLI-only, not serialized.
    #[serde(skip)]
    pub config_file: Option<PathBuf>,
}

impl Default for CleanConfig {
    fn default() -> Self {
        Self {
            path: ".".to_string(),
            patterns: vec![],
            exclude_patterns: constants::get_default_excludes(),
            dry_run: false,
            skip_confirmation: false,
            include_symlinks: false,
            remove_broken_symlinks: false,
            stats_mode: false,
            older_than_secs: None,
            show_progress: false,
            json_mode: false,
            protected_dirs: constants::get_protected_dirs(),
            build_artifacts: false,
            dependencies: false,
            larger_than_bytes: None,
            config_file: None,
        }
    }
}

impl CleanConfig {
    /// Start building a new configuration
    pub fn builder() -> CleanConfigBuilder {
        CleanConfigBuilder::default()
    }
}

/// Builder for constructing [`CleanConfig`] with a fluent API.
///
/// Obtained via [`CleanConfig::builder()`].
#[derive(Default)]
pub struct CleanConfigBuilder {
    config: CleanConfig,
}

impl CleanConfigBuilder {
    /// Set the root directory to clean.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.config.path = path.into();
        self
    }

    /// Set the glob patterns to match for deletion.
    pub fn patterns(mut self, patterns: Vec<String>) -> Self {
        self.config.patterns = patterns;
        self
    }

    /// Set glob patterns to exclude, replacing the defaults.
    pub fn exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.config.exclude_patterns = patterns;
        self
    }

    /// Enable or disable dry-run mode.
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.config.dry_run = dry_run;
        self
    }

    /// Enable or disable skipping the confirmation prompt.
    pub fn skip_confirmation(mut self, skip: bool) -> Self {
        self.config.skip_confirmation = skip;
        self
    }

    /// Enable or disable removal of matched symlinks.
    pub fn include_symlinks(mut self, include: bool) -> Self {
        self.config.include_symlinks = include;
        self
    }

    /// Enable or disable removal of broken symlinks.
    pub fn remove_broken_symlinks(mut self, remove: bool) -> Self {
        self.config.remove_broken_symlinks = remove;
        self
    }

    /// Enable or disable per-pattern statistics.
    pub fn stats_mode(mut self, stats: bool) -> Self {
        self.config.stats_mode = stats;
        self
    }

    /// Set the minimum age (in seconds) for files to be eligible for removal.
    pub fn older_than_secs(mut self, secs: Option<u64>) -> Self {
        self.config.older_than_secs = secs;
        self
    }

    /// Enable or disable the progress spinner during scanning.
    pub fn show_progress(mut self, progress: bool) -> Self {
        self.config.show_progress = progress;
        self
    }

    /// Enable or disable JSON output mode.
    pub fn json_mode(mut self, json: bool) -> Self {
        self.config.json_mode = json;
        self
    }

    /// Set the directory names that are never matched or entered.
    /// An empty list disables the protection.
    pub fn protected_dirs(mut self, dirs: Vec<String>) -> Self {
        self.config.protected_dirs = dirs;
        self
    }

    /// Enable or disable matching of build output directories.
    pub fn build_artifacts(mut self, enabled: bool) -> Self {
        self.config.build_artifacts = enabled;
        self
    }

    /// Enable or disable matching of dependency trees.
    pub fn dependencies(mut self, enabled: bool) -> Self {
        self.config.dependencies = enabled;
        self
    }

    /// Set the minimum size (in bytes) for targets to be eligible for removal.
    pub fn larger_than_bytes(mut self, bytes: Option<u64>) -> Self {
        self.config.larger_than_bytes = bytes;
        self
    }

    /// Consume the builder and return the finished [`CleanConfig`].
    pub fn build(self) -> CleanConfig {
        self.config
    }
}

// --------------------------------------------------------------------
// core

/// Pre-compiled pattern matchers for statistics attribution
type PatternMatchers = Vec<(String, GlobMatcher)>;

/// Configured excludes, then the built-in ones
type Excludes = (Option<GlobSet>, Option<GlobSet>);

/// A single matched item, used in JSON output.
#[derive(Serialize, Debug)]
pub struct MatchedItem {
    /// Display path of the matched file or directory.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// The glob pattern that matched this item, or the reason's name when
    /// no pattern did.
    pub pattern: String,
    /// Why this item was matched.
    pub reason: Reason,
    /// `directory`, `file` or `symlink`. A symlink is removed without being
    /// followed.
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// For a dependency tree, the command that restores it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore: Option<&'static str>,
}

/// A matched path awaiting sizing, reporting and removal.
struct Target {
    path: PathBuf,
    metadata: Metadata,
    pattern: String,
    reason: Reason,
    restore: Option<&'static str>,
    size: u64,
    is_dir: bool,
    /// Whether a directory holds an entry newer than the `older_than_secs` cutoff
    recent: bool,
}

/// Visit a growing set of work items across threads.
///
/// `visit` is called once per item and returns the items to visit next. The unit
/// of work is one directory listing, so one deep tree spreads over the available
/// cores as well as many shallow ones do.
fn parallel_dir_queue<T, F>(initial: Vec<T>, visit: F)
where
    T: Send,
    F: Fn(T) -> Vec<T> + Sync,
{
    if initial.is_empty() {
        return;
    }

    struct Queue<T> {
        pending: Vec<T>,
        /// Items taken but not yet finished, which may still produce more work
        active: usize,
    }
    let queue = Mutex::new(Queue {
        pending: initial,
        active: 0,
    });
    let ready = Condvar::new();

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let item = {
                    let mut guard = queue.lock().unwrap();
                    loop {
                        if let Some(item) = guard.pending.pop() {
                            guard.active += 1;
                            break Some(item);
                        }
                        if guard.active == 0 {
                            break None;
                        }
                        guard = ready.wait(guard).unwrap();
                    }
                };

                let Some(item) = item else {
                    // No work left and nobody can produce more: release the rest
                    ready.notify_all();
                    return;
                };

                let mut next = visit(item);

                let mut guard = queue.lock().unwrap();
                guard.pending.append(&mut next);
                guard.active -= 1;
                ready.notify_all();
            });
        }
    });
}

/// Paths that could not be read, with the reason.
type Warnings = Mutex<Vec<(PathBuf, String)>>;

/// Whether `meta` was modified after `cutoff`. An unreadable timestamp counts
/// as recent, so a target whose age cannot be shown is kept.
fn is_recent(meta: &Metadata, cutoff: SystemTime) -> bool {
    meta.modified().map_or(true, |modified| modified > cutoff)
}

/// Record a path that could not be read
fn record(warnings: &Warnings, path: &Path, reason: impl std::fmt::Display) {
    warn!("Cannot read {:?}: {}", path.display(), reason);
    warnings
        .lock()
        .unwrap()
        .push((path.to_path_buf(), reason.to_string()));
}

/// Total the sizes of every regular file under each directory in `dirs`, and
/// report whether each holds an entry modified after `cutoff`.
///
/// Symlinks are counted at their own size and never followed, matching what
/// `remove_dir_all` will actually delete and ruling out a symlink cycle walking
/// forever. A subtree that cannot be read counts as recent, since its age is
/// unknown.
fn parallel_dir_sizes(
    dirs: &[PathBuf],
    cutoff: Option<SystemTime>,
    warnings: &Warnings,
) -> Vec<(u64, bool)> {
    if dirs.is_empty() {
        return Vec::new();
    }

    let totals: Vec<AtomicU64> = dirs.iter().map(|_| AtomicU64::new(0)).collect();
    let recent: Vec<AtomicBool> = dirs.iter().map(|_| AtomicBool::new(false)).collect();

    // Each work item carries the index of the matched directory it belongs to
    parallel_dir_queue(
        dirs.iter().cloned().enumerate().collect(),
        |(owner, dir): (usize, PathBuf)| {
            // A recent target will not be removed, so its size no longer matters
            if cutoff.is_some() && recent[owner].load(Ordering::Relaxed) {
                return Vec::new();
            }
            let mut bytes = 0u64;
            let mut is_recent_here = false;
            let mut subdirs = Vec::new();
            match fs::read_dir(&dir) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = match entry {
                            Ok(entry) => entry,
                            Err(e) => {
                                record(warnings, &dir, e);
                                is_recent_here = true;
                                continue;
                            }
                        };
                        let is_dir = match entry.file_type() {
                            Ok(ty) => ty.is_dir(),
                            Err(e) => {
                                record(warnings, &entry.path(), e);
                                is_recent_here = true;
                                continue;
                            }
                        };
                        if is_dir {
                            subdirs.push((owner, entry.path()));
                            if cutoff.is_none() {
                                continue;
                            }
                        }
                        // `DirEntry::metadata` does not follow symlinks
                        match entry.metadata() {
                            Ok(meta) => {
                                if !is_dir {
                                    bytes += meta.len();
                                }
                                if cutoff.is_some_and(|c| is_recent(&meta, c)) {
                                    is_recent_here = true;
                                }
                            }
                            Err(e) => {
                                record(warnings, &entry.path(), e);
                                is_recent_here = true;
                            }
                        }
                    }
                }
                Err(e) => {
                    record(warnings, &dir, e);
                    is_recent_here = true;
                }
            }
            if bytes > 0 {
                totals[owner].fetch_add(bytes, Ordering::Relaxed);
            }
            if is_recent_here {
                recent[owner].store(true, Ordering::Relaxed);
            }
            subdirs
        },
    );

    totals
        .into_iter()
        .zip(recent)
        .map(|(t, r)| (t.into_inner(), r.into_inner()))
        .collect()
}

/// Whether `current` is the same filesystem object as `scanned`.
#[cfg(unix)]
fn same_object(scanned: &Metadata, current: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    // An inode number can be reused once freed, so the type is compared too
    scanned.dev() == current.dev()
        && scanned.ino() == current.ino()
        && scanned.file_type() == current.file_type()
}

/// Whether `current` is the same filesystem object as `scanned`. Without a
/// stable file identity in std, only the type can be compared.
#[cfg(not(unix))]
fn same_object(scanned: &Metadata, current: &Metadata) -> bool {
    scanned.file_type() == current.file_type()
}

/// Skip the current and parent directory references, and anything reached
/// through `..`.
fn should_process(entry_path: &Path) -> bool {
    let current_path = Path::new(".");
    let parent_path = Path::new("..");

    if entry_path == current_path || entry_path == parent_path {
        return false;
    }

    if entry_path.starts_with("..") {
        warn!("skipping {:?}", entry_path.display());
        return false;
    }

    true
}

/// Whether `path` is build output at the top level of a project.
///
/// The directory name must be paired with a marker file in
/// [`constants::BUILD_ARTIFACTS`], and both that marker and `.git` must sit in
/// the parent directory. `.git` is what pins the match to the project root: a
/// CMake subdirectory carries its own `CMakeLists.txt`, so the marker alone
/// would claim `src/program/build` too.
///
/// The project must also be the outermost one below `root`. A submodule or a
/// vendored checkout carries its own `.git` and marker, so it would otherwise
/// qualify on its own.
fn is_artifact_dir(path: &Path, name: &OsStr, root: &Path) -> bool {
    // The name test is a few string compares; each marker test below is a stat.
    if !constants::BUILD_ARTIFACTS
        .iter()
        .any(|(dir, _)| OsStr::new(dir) == name)
    {
        return false;
    }

    let Some(project) = path.parent() else {
        return false;
    };

    // `.git` is a file in a submodule checkout, so any entry type counts
    if !project.join(".git").exists() {
        return false;
    }

    if !constants::BUILD_ARTIFACTS
        .iter()
        .any(|(dir, marker)| OsStr::new(dir) == name && project.join(marker).is_file())
    {
        return false;
    }

    // Any `.git` between the project and `root`, `root` included, makes this a
    // nested repository. An unreadable entry counts as present.
    let mut ancestor = project;
    while ancestor != root {
        let Some(parent) = ancestor.parent() else {
            break;
        };
        ancestor = parent;
        match fs::symlink_metadata(ancestor.join(".git")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            _ => return false,
        }
    }
    true
}

/// The restore command for `path` when it is a dependency tree: its name is in
/// [`constants::DEPENDENCIES`] and the paired lock file sits beside it.
fn dependency_restore(path: &Path, name: &OsStr) -> Option<&'static str> {
    let parent = path.parent()?;
    constants::DEPENDENCIES
        .iter()
        .find(|(dir, marker, _)| {
            // A lock reached through a symlink says nothing about this tree
            OsStr::new(dir) == name
                && fs::symlink_metadata(parent.join(marker)).is_ok_and(|m| m.is_file())
        })
        .map(|(_, _, restore)| *restore)
}

/// Find which pattern matched the entry using pre-compiled matchers
fn find_matching_pattern(matchers: &[(String, GlobMatcher)], entry_path: &Path) -> Option<String> {
    for (pattern, matcher) in matchers {
        if matcher.is_match(entry_path) {
            return Some(pattern.clone());
        }
    }
    None
}

/// Traversal state shared by the walker threads.
struct Scan<'a> {
    config: &'a CleanConfig,
    base_path: &'a Path,
    /// The root as given, which the paths the walk produces start with
    root: &'a Path,
    include_set: &'a GlobSet,
    exclude_set: &'a Option<GlobSet>,
    /// The built-in excludes, which do not apply to a dependency tree
    default_exclude_set: &'a Option<GlobSet>,
    matchers: &'a [(String, GlobMatcher)],
    progress: Option<ProgressBar>,
    processed: AtomicU64,
    collected: Mutex<Vec<Target>>,
    warnings: Warnings,
}

impl Scan<'_> {
    /// Record a path the walk could not read, through the progress bar when one is drawn
    fn skip(&self, path: &Path, reason: impl std::fmt::Display) {
        match self.progress {
            Some(ref pb) => {
                pb.println(format!("Cannot read {:?}: {}", path.display(), reason));
                self.warnings
                    .lock()
                    .unwrap()
                    .push((path.to_path_buf(), reason.to_string()));
            }
            None => record(&self.warnings, path, reason),
        }
    }

    /// Report a line, through the progress bar when one is drawn
    fn note(&self, msg: String) {
        if let Some(ref pb) = self.progress {
            pb.println(&msg);
        } else {
            info!("{}", msg);
        }
    }

    /// List one directory, collecting what matches and returning what to descend into.
    fn visit_dir(&self, dir: PathBuf) -> Vec<PathBuf> {
        // The walk never follows symlinks, so an entry can only escape `base_path`
        // through a symlinked ancestor. Checking each directory once therefore
        // covers every entry in it, for one `realpath` per directory rather than
        // one per match.
        match dir.canonicalize() {
            Ok(resolved) if resolved.starts_with(self.base_path) => {}
            Ok(_) => {
                self.skip(&dir, "resolves outside the working directory");
                return Vec::new();
            }
            // A directory that will not resolve cannot be shown to be inside
            // `base_path`, so it is skipped rather than walked.
            Err(e) => {
                self.skip(&dir, e);
                return Vec::new();
            }
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => {
                self.skip(&dir, e);
                return Vec::new();
            }
        };

        let mut subdirs = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.skip(&dir, e);
                    continue;
                }
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(e) => {
                    self.skip(&path, e);
                    continue;
                }
            };
            if self.consider(&path, Some(&entry.file_name()), file_type) {
                subdirs.push(path);
            }
        }
        subdirs
    }

    /// Decide what to do with one entry, returning whether to descend into it.
    ///
    /// A `name` of `None` marks the root, which protection does not apply to.
    fn consider(&self, path: &Path, name: Option<&OsStr>, file_type: fs::FileType) -> bool {
        let processed = self.processed.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(ref pb) = self.progress {
            if processed.is_multiple_of(100) {
                pb.set_message(format!(
                    "Scanned {} items, found {} matches",
                    processed,
                    self.collected.lock().unwrap().len()
                ));
            }
        }

        let is_root = name.is_none();

        if !should_process(path) {
            // For the root this only rules out matching, never traversal: the
            // default path is `.`, which `should_process` rejects by name.
            return is_root && file_type.is_dir();
        }

        // Protected names are neither matched nor entered
        if let Some(name) = name {
            if self
                .config
                .protected_dirs
                .iter()
                .any(|d| OsStr::new(d) == name)
            {
                debug!("Protected, skipping: {:?}", path.display());
                return false;
            }
        }

        let is_dir = file_type.is_dir();
        let is_symlink = file_type.is_symlink();

        // Excluded entries are neither matched nor entered. The check runs ahead of
        // the include match so that excluding a directory prunes the walk: a
        // virtualenv is skipped in one step rather than scanned and rejected file by
        // file. The root is exempt, as pointing reclean at an excluded directory is
        // deliberate.
        let excluded =
            |set: &Option<GlobSet>| !is_root && set.as_ref().is_some_and(|set| set.is_match(path));
        if excluded(self.exclude_set) {
            self.note(format!("Excluded: {:?}", path.display()));
            return false;
        }

        // A dependency tree is checked ahead of the built-in excludes: `.venv` is
        // excluded to save scanning it, and a lock beside it asks for it whole.
        if self.config.dependencies && is_dir {
            if let Some(restore) = name.and_then(|n| dependency_restore(path, n)) {
                self.collect(path, Reason::Dependency, Some(restore), String::new());
                return false;
            }
        }
        if excluded(self.default_exclude_set) {
            self.note(format!("Excluded: {:?}", path.display()));
            return false;
        }

        // Handle broken symlinks
        if self.config.remove_broken_symlinks && is_symlink && fs::metadata(path).is_err() {
            self.collect(path, Reason::BrokenSymlink, None, String::new());
            return false;
        }

        // A pattern match is the cheaper reason to report, so build output is
        // only looked for when no pattern matched.
        let reason = if self.include_set.is_match(path) {
            Reason::Pattern
        } else if self.config.build_artifacts
            && is_dir
            && name.is_some_and(|n| is_artifact_dir(path, n, self.root))
        {
            Reason::BuildArtifact
        } else {
            return is_dir;
        };

        // Skip symlinks unless explicitly included
        if is_symlink && !self.config.include_symlinks {
            return false;
        }

        // Only stats and JSON output name the matching pattern, so the second
        // matcher pass is skipped when neither is on.
        let pattern =
            if reason == Reason::Pattern && (self.config.stats_mode || self.config.json_mode) {
                find_matching_pattern(self.matchers, path).unwrap_or_default()
            } else {
                String::new()
            };

        self.collect(path, reason, None, pattern);

        // A matched directory is claimed whole. Descending into it again counts its
        // contents a second time, which is what inflated both the item count and the
        // byte total shown before the confirmation prompt.
        false
    }

    /// Record a matched entry as a target. Age is checked after sizing, since a
    /// directory's age depends on its contents.
    fn collect(&self, path: &Path, reason: Reason, restore: Option<&'static str>, pattern: String) {
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                self.skip(path, e);
                return;
            }
        };

        let is_dir = metadata.is_dir();
        // Directories are sized afterwards, in parallel
        let size = if is_dir { 0 } else { metadata.len() };

        self.collected.lock().unwrap().push(Target {
            path: path.to_path_buf(),
            metadata,
            // Targets no pattern matched are attributed to their reason
            pattern: if pattern.is_empty() && reason != Reason::Pattern {
                reason.name().to_string()
            } else {
                pattern
            },
            reason,
            restore,
            size,
            is_dir,
            recent: false,
        });
    }
}

/// Runtime executor for cleaning jobs.
///
/// Holds a [`CleanConfig`] plus transient state accumulated during a run
/// (matched targets, statistics, failures).
pub struct CleaningJob {
    /// The configuration driving this job.
    pub config: CleanConfig,
    targets: Vec<Target>,
    /// Cumulative size in bytes of all matched items.
    pub size: u64,
    /// Number of matched items.
    pub counter: usize,
    /// Per-pattern statistics: pattern -> (count, total bytes).
    pub stats: HashMap<String, (usize, u64)>,
    /// Per-reason statistics: reason -> (count, total bytes), cheapest to restore first.
    pub reasons: BTreeMap<Reason, (usize, u64)>,
    /// Paths that could not be deleted, with error messages.
    pub failed_deletions: Vec<(PathBuf, String)>,
    /// Matched items collected for JSON output.
    pub matched_items: Vec<MatchedItem>,
    /// Paths that could not be read during the scan, with error messages.
    /// Matches beneath them were not considered.
    pub warnings: Vec<(PathBuf, String)>,
}

impl CleaningJob {
    /// Create a new cleaning job from a configuration
    pub fn new(config: CleanConfig) -> Self {
        Self {
            config,
            targets: Vec::new(),
            size: 0,
            counter: 0,
            stats: HashMap::new(),
            reasons: BTreeMap::new(),
            failed_deletions: Vec::new(),
            matched_items: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Return whether any deletions failed
    pub fn has_failures(&self) -> bool {
        !self.failed_deletions.is_empty()
    }

    /// Return whether any part of the tree could not be read
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Produce JSON output summarizing the run
    pub fn to_json(&self) -> std::result::Result<String, serde_json::Error> {
        let stats: Vec<_> = self
            .stats
            .iter()
            .map(|(pattern, (count, size))| {
                serde_json::json!({
                    "pattern": pattern,
                    "count": count,
                    "size": size,
                    "size_human": format_size(*size),
                })
            })
            .collect();

        let reasons: Vec<_> = self
            .reasons
            .iter()
            .map(|(reason, (count, size))| {
                serde_json::json!({
                    "reason": reason,
                    "count": count,
                    "size": size,
                })
            })
            .collect();

        let failures: Vec<_> = self
            .failed_deletions
            .iter()
            .map(|(path, err)| {
                serde_json::json!({
                    "path": path.display().to_string(),
                    "error": err,
                })
            })
            .collect();

        let warnings: Vec<_> = self
            .warnings
            .iter()
            .map(|(path, err)| {
                serde_json::json!({
                    "path": path.display().to_string(),
                    "error": err,
                })
            })
            .collect();

        // `schema` changes when a field changes meaning or goes, not when one is added
        let output = serde_json::json!({
            "schema": 1,
            "config": self.config.config_file.as_ref().map(|p| p.display().to_string()),
            "matches": self.matched_items,
            "summary": {
                "total_count": self.counter,
                "total_size": self.size,
                "total_size_human": format_size(self.size),
                "dry_run": self.config.dry_run,
            },
            "stats": stats,
            "reasons": reasons,
            "failures": failures,
            "warnings": warnings,
        });

        serde_json::to_string_pretty(&output)
    }

    /// Whether targets are removed without asking first.
    fn deletes_unprompted(&self) -> bool {
        self.config.skip_confirmation && !self.config.dry_run
    }

    /// Run the cleaning job
    #[time("info")]
    pub fn run(&mut self) -> Result<()> {
        let path_str = self.config.path.clone();
        let path = Path::new(&path_str);

        // Canonicalize base path for security checks. An unusable root is an I/O
        // failure rather than a configuration error: the CLI exits 1, not 2.
        let base_path = path.canonicalize().map_err(|e| {
            CleanError::IoError(std::io::Error::new(
                e.kind(),
                format!("Invalid path '{}': {}", path_str, e),
            ))
        })?;

        // Build globsets
        let (include_set, excludes, matchers) = self.build_globsets()?;

        // Collect targets
        self.collect_targets(path, &base_path, &include_set, &excludes, &matchers)?;

        // Size is known only after sizing, so this filter follows it
        let larger_than = self.config.larger_than_bytes;
        // `Some(None)` is an age older than any timestamp can express
        let cutoff = self
            .config
            .older_than_secs
            .map(|secs| SystemTime::now().checked_sub(Duration::from_secs(secs)));

        // Size matched directories, drop targets that are too recent, then
        // account for and report every target
        self.size_directories(cutoff.flatten());
        if let Some(cutoff) = cutoff {
            self.targets
                .retain(|t| cutoff.is_some_and(|c| !t.recent && !is_recent(&t.metadata, c)));
        }
        if let Some(limit) = larger_than {
            self.targets.retain(|t| t.size >= limit);
        }
        self.report_targets();

        // Display statistics if enabled (suppressed in JSON mode)
        if self.config.stats_mode && !self.config.json_mode {
            self.display_stats();
        }

        // Confirm deletion if needed. A dry run removes nothing, so it never asks.
        if !self.targets.is_empty()
            && !self.config.skip_confirmation
            && !self.config.dry_run
            && !confirm("Do you want to delete the above?")?
        {
            warn!("Cleaning operation cancelled.");
            return Ok(());
        }

        if !self.config.dry_run {
            self.execute_deletion(&base_path);
        }

        if self.has_warnings() && !self.config.json_mode {
            warn!(
                "{} path(s) could not be read; matches beneath them were not considered",
                self.warnings.len()
            );
        }

        Ok(())
    }

    /// Build globsets for include and exclude patterns, plus individual matchers for stats
    ///
    /// Excludes come back as two sets, the configured ones and the built-in
    /// [`constants::DEFAULT_EXCLUDES`], since only the first applies to a
    /// dependency tree.
    fn build_globsets(&self) -> Result<(GlobSet, Excludes, PatternMatchers)> {
        let mut builder = GlobSetBuilder::new();
        let mut matchers = Vec::new();

        for pattern in self.config.patterns.iter() {
            let glob = Glob::new(pattern)?;
            builder.add(glob.clone());
            matchers.push((pattern.clone(), glob.compile_matcher()));
        }
        let include_set = builder.build()?;

        let globset = |defaults: bool| -> Result<Option<GlobSet>> {
            let mut builder = GlobSetBuilder::new();
            let mut any = false;
            for pattern in self.config.exclude_patterns.iter() {
                if constants::DEFAULT_EXCLUDES.contains(&pattern.as_str()) == defaults {
                    builder.add(Glob::new(pattern)?);
                    any = true;
                }
            }
            Ok(if any { Some(builder.build()?) } else { None })
        };

        Ok((include_set, (globset(false)?, globset(true)?), matchers))
    }

    /// Collect targets for deletion
    fn collect_targets(
        &mut self,
        path: &Path,
        base_path: &Path,
        include_set: &GlobSet,
        excludes: &Excludes,
        matchers: &[(String, GlobMatcher)],
    ) -> Result<()> {
        // Create progress bar if requested
        let progress = if self.config.show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} [{elapsed_precise}] {msg}")
                    .unwrap(),
            );
            pb.set_message("Scanning files...");
            pb.enable_steady_tick(Duration::from_millis(100));
            Some(pb)
        } else {
            None
        };

        let scan = Scan {
            config: &self.config,
            base_path,
            root: path,
            include_set,
            exclude_set: &excludes.0,
            default_exclude_set: &excludes.1,
            matchers,
            progress,
            processed: AtomicU64::new(0),
            collected: Mutex::new(Vec::new()),
            warnings: Mutex::new(Vec::new()),
        };

        // The root is considered on its own, exempt from protection: pointing reclean
        // at `.git` is a deliberate act, and silently doing nothing there would be
        // its own trap.
        let root_type = fs::symlink_metadata(path)
            .map_err(|e| {
                CleanError::IoError(std::io::Error::new(
                    e.kind(),
                    format!("Cannot read {:?}: {}", path, e),
                ))
            })?
            .file_type();

        if scan.consider(path, None, root_type) {
            parallel_dir_queue(vec![path.to_path_buf()], |dir| scan.visit_dir(dir));
        }

        let Scan {
            progress,
            processed,
            collected,
            warnings,
            ..
        } = scan;

        self.targets = collected.into_inner().unwrap();
        self.warnings = warnings.into_inner().unwrap();
        // Threads finish in no fixed order, so the listing is sorted to keep a run
        // reproducible and the pre-confirmation output readable.
        self.targets.sort_by(|a, b| a.path.cmp(&b.path));

        // Finish progress bar
        if let Some(pb) = progress {
            pb.finish_with_message(format!(
                "Scan complete: {} items scanned, {} matches found",
                processed.into_inner(),
                self.targets.len()
            ));
        }

        Ok(())
    }

    /// Fill in the size of every matched directory
    fn size_directories(&mut self, cutoff: Option<SystemTime>) {
        let indices: Vec<usize> = self
            .targets
            .iter()
            .enumerate()
            .filter(|(_, t)| t.is_dir)
            .map(|(i, _)| i)
            .collect();

        let dirs: Vec<PathBuf> = indices
            .iter()
            .map(|&i| self.targets[i].path.clone())
            .collect();

        let warnings = Mutex::new(Vec::new());
        let sized = parallel_dir_sizes(&dirs, cutoff, &warnings);
        for (&index, (size, recent)) in indices.iter().zip(sized) {
            self.targets[index].size = size;
            self.targets[index].recent = recent;
        }
        self.warnings.append(&mut warnings.into_inner().unwrap());
    }

    /// Accumulate totals, statistics and per-item output for the collected targets
    fn report_targets(&mut self) {
        // Targets deleted without a prompt are reported by `execute_deletion` instead,
        // so a run does not list the same path twice.
        let announce = !self.deletes_unprompted() && !self.config.json_mode;

        for target in self.targets.iter() {
            self.size += target.size;
            self.counter += 1;

            let reason = self.reasons.entry(target.reason).or_insert((0, 0));
            reason.0 += 1;
            reason.1 += target.size;

            if self.config.stats_mode {
                let stat = self.stats.entry(target.pattern.clone()).or_insert((0, 0));
                stat.0 += 1;
                stat.1 += target.size;
            }

            if self.config.json_mode {
                self.matched_items.push(MatchedItem {
                    path: target.path.display().to_string(),
                    size: target.size,
                    pattern: target.pattern.clone(),
                    reason: target.reason,
                    kind: if target.is_dir {
                        "directory"
                    } else if target.metadata.is_symlink() {
                        "symlink"
                    } else {
                        "file"
                    },
                    restore: target.restore,
                });
            }

            if announce {
                match target.restore {
                    Some(restore) => info!(
                        "Matched: {:?}  {}  (restore: {})",
                        target.path.display(),
                        format_size(target.size),
                        restore
                    ),
                    None => info!(
                        "Matched: {:?}  {}",
                        target.path.display(),
                        format_size(target.size)
                    ),
                }
            }
        }

        if self.config.json_mode || self.counter == 0 {
            return;
        }
        info!(
            "{} target(s), {} to reclaim",
            self.counter,
            format_size(self.size)
        );
        // One reason would repeat the total
        if self.reasons.len() > 1 {
            for (reason, (count, size)) in &self.reasons {
                info!(
                    "  {:<16}{:>6}  {:>12}",
                    reason.name(),
                    count,
                    format_size(*size)
                );
            }
        }
    }

    /// Remove every collected target in parallel, reporting in list order
    fn execute_deletion(&mut self, base_path: &Path) {
        let targets = std::mem::take(&mut self.targets);
        let announce = self.deletes_unprompted() && !self.config.json_mode;

        // No target can sit inside another: the walk stops descending at a matched
        // directory, so a child of one is never collected. Removals therefore
        // never race each other down one subtree.
        let results: Vec<OnceLock<std::result::Result<(), String>>> =
            targets.iter().map(|_| OnceLock::new()).collect();
        let next = AtomicUsize::new(0);
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(targets.len());
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(target) = targets.get(i) else {
                        return;
                    };
                    let _ =
                        results[i].set(remove_target(&target.path, &target.metadata, base_path));
                });
            }
        });

        let (mut removed, mut reclaimed) = (0usize, 0u64);
        for (target, result) in targets.iter().zip(results) {
            match result.into_inner() {
                Some(Ok(())) => {
                    removed += 1;
                    reclaimed += target.size;
                    if announce {
                        info!("Deleted: {:?}", target.path.display());
                    }
                }
                Some(Err(e)) => {
                    error!("Failed to remove {:?}: {}", target.path.display(), e);
                    self.failed_deletions.push((target.path.clone(), e));
                }
                None => unreachable!("every target is visited"),
            }
        }

        if !self.config.json_mode && removed > 0 {
            info!(
                "Deleted {} item(s) totalling {}",
                removed,
                format_size(reclaimed)
            );
        }
        if !self.failed_deletions.is_empty() {
            error!("{} removal(s) failed", self.failed_deletions.len());
        }
    }

    /// Display statistics about matches
    fn display_stats(&self) {
        if !self.config.stats_mode {
            return;
        }

        info!("\n=== Statistics ===");
        let mut patterns: Vec<_> = self.stats.iter().collect();
        patterns.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count)); // count descending

        for (pattern, (count, size)) in patterns {
            info!("  {}: {} item(s), {}", pattern, count, format_size(*size));
        }
        info!("==================\n");
    }
}

/// Why `path` must not be removed: it no longer resolves inside `base_path`,
/// or it is not the object the scan found there.
///
/// This narrows the window between the scan and the removal to adjacent
/// syscalls. It does not close it: the removal resolves `path` again.
fn changed_since_scan(path: &Path, scanned: &Metadata, base_path: &Path) -> Option<String> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let Some(name) = path.file_name() else {
        return Some("has no file name".to_string());
    };
    match parent.canonicalize() {
        Ok(resolved) if resolved.join(name).starts_with(base_path) => {}
        Ok(_) => return Some("now resolves outside the working directory".to_string()),
        Err(e) => return Some(format!("cannot resolve parent: {}", e)),
    }
    match fs::symlink_metadata(path) {
        Ok(current) if same_object(scanned, &current) => None,
        Ok(_) => Some("replaced since it was scanned".to_string()),
        Err(e) => Some(format!("cannot re-check: {}", e)),
    }
}

/// Remove one target, unless it changed since the scan
fn remove_target(
    path: &Path,
    metadata: &Metadata,
    base_path: &Path,
) -> std::result::Result<(), String> {
    if let Some(reason) = changed_since_scan(path, metadata, base_path) {
        return Err(reason);
    }
    let result = if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else if metadata.is_file() || metadata.is_symlink() {
        fs::remove_file(path)
    } else {
        return Err("unsupported file type".to_string());
    };
    result.map_err(|e| e.to_string())
}

/// Ask a yes/no question on the terminal, or read the answer from piped stdin.
///
/// Only `y` or `Y` proceeds; anything else, end of input included, cancels.
fn confirm(prompt: &str) -> Result<bool> {
    use std::io::{IsTerminal, Read, Write};

    if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        // A key pressed before the prompt appeared must not answer it
        #[cfg(unix)]
        // SAFETY: tcflush only discards pending input on a valid descriptor.
        unsafe {
            libc::tcflush(libc::STDIN_FILENO, libc::TCIFLUSH);
        }
        return Confirm::new()
            .with_prompt(prompt)
            .interact()
            .map_err(|e| CleanError::IoError(std::io::Error::other(e.to_string())));
    }

    eprint!("{} [y/N] ", prompt);
    let _ = std::io::stderr().flush();
    let mut answer = [0u8; 1];
    Ok(
        matches!(std::io::stdin().read(&mut answer), Ok(1) if answer[0] == b'y' || answer[0] == b'Y'),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaced_file_is_not_removed() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let base = temp_dir.path().canonicalize().unwrap();
        let path = base.join("a.pyc");
        fs::write(&path, "scanned").unwrap();
        let scanned = fs::symlink_metadata(&path).unwrap();

        // Keep the old inode linked so the replacement cannot reuse its number
        fs::rename(&path, base.join("kept")).unwrap();
        fs::write(&path, "replacement").unwrap();

        assert!(remove_target(&path, &scanned, &base).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    }

    #[test]
    #[cfg(unix)]
    fn directory_replaced_by_symlink_is_not_followed() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let base = temp_dir.path().canonicalize().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        fs::write(outside.path().join("precious"), "x").unwrap();

        let dir = base.join("sub");
        let target = dir.join("__pycache__");
        fs::create_dir_all(&target).unwrap();
        let scanned = fs::symlink_metadata(&target).unwrap();

        // An ancestor swapped for a link to a tree outside the root
        fs::remove_dir_all(&dir).unwrap();
        fs::create_dir(outside.path().join("__pycache__")).unwrap();
        std::os::unix::fs::symlink(outside.path(), &dir).unwrap();

        assert!(remove_target(&target, &scanned, &base).is_err());
        assert!(outside.path().join("__pycache__").exists());
        assert!(outside.path().join("precious").exists());
    }

    #[test]
    fn unchanged_target_is_removed() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let base = temp_dir.path().canonicalize().unwrap();
        let path = base.join("a.pyc");
        fs::write(&path, "x").unwrap();
        let scanned = fs::symlink_metadata(&path).unwrap();

        assert!(remove_target(&path, &scanned, &base).is_ok());
        assert!(!path.exists());
    }
}
