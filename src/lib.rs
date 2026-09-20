//! Safe, pattern-based file and directory cleaner.
//!
//! `rclean` recursively removes files and directories matching glob patterns,
//! with built-in safety measures including dry-run mode, confirmation prompts,
//! path traversal protection, and symlink guards.
//!
//! # Quick start
//!
//! ```no_run
//! use rclean::{CleanConfig, CleaningJob};
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
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
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

    if duration.len() < 2 {
        return Err(CleanError::ConfigError(format!(
            "Invalid duration '{}': must be a number followed by a unit (s, m, h, d, w)",
            duration
        )));
    }

    let (num_part, unit_part) = duration.split_at(duration.len() - 1);
    let number: u64 = num_part.parse().map_err(|_| {
        CleanError::ConfigError(format!("Invalid number in duration: {}", num_part))
    })?;

    let multiplier = match unit_part {
        "s" => 1,      // seconds
        "m" => 60,     // minutes
        "h" => 3600,   // hours
        "d" => 86400,  // days
        "w" => 604800, // weeks
        _ => {
            return Err(CleanError::ConfigError(format!(
                "Invalid duration unit '{}'. Use 's', 'm', 'h', 'd', or 'w'",
                unit_part
            )))
        }
    };

    Ok(number * multiplier)
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
/// Checks `~/.config/rclean/config.toml`.
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("rclean").join("config.toml"))
        .filter(|p| p.is_file())
}

/// Discover a config file: first search upward for `.rclean.toml`, then fall back to global.
pub fn discover_config(start_dir: &Path) -> Option<PathBuf> {
    find_config_upward(start_dir, constants::SETTINGS_FILENAME).or_else(global_config_path)
}

// --------------------------------------------------------------------
// configuration

/// Serializable configuration for a cleaning job.
///
/// Construct via [`CleanConfig::builder()`] or deserialize from a `.rclean.toml` file.
#[derive(Serialize, Deserialize, Clone, Debug)]
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

    /// Consume the builder and return the finished [`CleanConfig`].
    pub fn build(self) -> CleanConfig {
        self.config
    }
}

// --------------------------------------------------------------------
// core

/// Pre-compiled pattern matchers for statistics attribution
type PatternMatchers = Vec<(String, GlobMatcher)>;

/// A single matched item, used in JSON output.
#[derive(Serialize, Debug)]
pub struct MatchedItem {
    /// Display path of the matched file or directory.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// The glob pattern that matched this item.
    pub pattern: String,
}

/// A matched path awaiting sizing, reporting and removal.
struct Target {
    path: PathBuf,
    metadata: Metadata,
    pattern: String,
    size: u64,
    is_dir: bool,
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

/// Total the sizes of every regular file under each directory in `dirs`.
///
/// Symlinks are counted at their own size and never followed, matching what
/// `remove_dir_all` will actually delete and ruling out a symlink cycle walking
/// forever.
fn parallel_dir_sizes(dirs: &[PathBuf]) -> Vec<u64> {
    if dirs.is_empty() {
        return Vec::new();
    }

    let totals: Vec<AtomicU64> = dirs.iter().map(|_| AtomicU64::new(0)).collect();

    // Each work item carries the index of the matched directory it belongs to
    parallel_dir_queue(
        dirs.iter().cloned().enumerate().collect(),
        |(owner, dir): (usize, PathBuf)| {
            let mut bytes = 0u64;
            let mut subdirs = Vec::new();
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    match entry.file_type() {
                        Ok(ty) if ty.is_dir() => subdirs.push((owner, entry.path())),
                        Ok(_) => {
                            if let Ok(meta) = entry.metadata() {
                                bytes += meta.len();
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            if bytes > 0 {
                totals[owner].fetch_add(bytes, Ordering::Relaxed);
            }
            subdirs
        },
    );

    totals.into_iter().map(|t| t.into_inner()).collect()
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
fn is_artifact_dir(path: &Path, name: &OsStr) -> bool {
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

    constants::BUILD_ARTIFACTS
        .iter()
        .any(|(dir, marker)| OsStr::new(dir) == name && project.join(marker).is_file())
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
    include_set: &'a GlobSet,
    exclude_set: &'a Option<GlobSet>,
    matchers: &'a [(String, GlobMatcher)],
    progress: Option<ProgressBar>,
    processed: AtomicU64,
    collected: Mutex<Vec<Target>>,
}

impl Scan<'_> {
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
                warn!(
                    "Skipping path outside working directory: {:?}",
                    dir.display()
                );
                return Vec::new();
            }
            // A directory that will not resolve cannot be shown to be inside
            // `base_path`, so it is skipped rather than walked.
            Err(e) => {
                warn!("Skipping unresolvable path {:?}: {}", dir.display(), e);
                return Vec::new();
            }
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => return Vec::new(),
        };

        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
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
        // file. The root is exempt, as pointing rclean at an excluded directory is
        // deliberate.
        if !is_root {
            if let Some(ref exclude) = self.exclude_set {
                if exclude.is_match(path) {
                    self.note(format!("Excluded: {:?}", path.display()));
                    return false;
                }
            }
        }

        // Handle broken symlinks
        if self.config.remove_broken_symlinks && is_symlink && fs::metadata(path).is_err() {
            self.collect(path, "broken-symlink".to_string());
            return false;
        }

        // Check if path matches include patterns, or is build output when asked for
        let artifact =
            self.config.build_artifacts && is_dir && name.is_some_and(|n| is_artifact_dir(path, n));

        if !artifact && !self.include_set.is_match(path) {
            return is_dir;
        }

        // Skip symlinks unless explicitly included
        if is_symlink && !self.config.include_symlinks {
            return false;
        }

        // Only stats and JSON output name the matching pattern, so the second
        // matcher pass is skipped when neither is on.
        let pattern = if self.config.stats_mode || self.config.json_mode {
            find_matching_pattern(self.matchers, path).unwrap_or_else(|| {
                if artifact {
                    "build-artifact"
                } else {
                    "unknown"
                }
                .to_string()
            })
        } else {
            String::new()
        };

        self.collect(path, pattern);

        // A matched directory is claimed whole. Descending into it again counts its
        // contents a second time, which is what inflated both the item count and the
        // byte total shown before the confirmation prompt.
        false
    }

    /// Record a matched entry as a target, unless it is younger than `older_than_secs`
    fn collect(&self, path: &Path, pattern: String) {
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to get metadata for {:?}: {}", path.display(), e);
                return;
            }
        };

        // Check age-based filtering
        if let Some(older_than_secs) = self.config.older_than_secs {
            if let Ok(modified) = metadata.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    if elapsed.as_secs() < older_than_secs {
                        // File is too new, skip it
                        return;
                    }
                }
            }
        }

        let is_dir = metadata.is_dir();
        // Directories are sized afterwards, in parallel
        let size = if is_dir { 0 } else { metadata.len() };

        self.collected.lock().unwrap().push(Target {
            path: path.to_path_buf(),
            metadata,
            pattern,
            size,
            is_dir,
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
    /// Paths that could not be deleted, with error messages.
    pub failed_deletions: Vec<(PathBuf, String)>,
    /// Matched items collected for JSON output.
    pub matched_items: Vec<MatchedItem>,
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
            failed_deletions: Vec::new(),
            matched_items: Vec::new(),
        }
    }

    /// Return whether any deletions failed
    pub fn has_failures(&self) -> bool {
        !self.failed_deletions.is_empty()
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

        let output = serde_json::json!({
            "matches": self.matched_items,
            "summary": {
                "total_count": self.counter,
                "total_size": self.size,
                "total_size_human": format_size(self.size),
                "dry_run": self.config.dry_run,
            },
            "stats": stats,
            "failures": failures,
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

        // Canonicalize base path for security checks
        let base_path = path
            .canonicalize()
            .map_err(|e| CleanError::ConfigError(format!("Invalid path '{}': {}", path_str, e)))?;

        // Build globsets
        let (include_set, exclude_set, matchers) = self.build_globsets()?;

        // Collect targets
        self.collect_targets(path, &base_path, &include_set, &exclude_set, &matchers)?;

        // Size matched directories, then account for and report every target
        self.size_directories();
        self.report_targets();

        // Display statistics if enabled (suppressed in JSON mode)
        if self.config.stats_mode && !self.config.json_mode {
            self.display_stats();
        }

        // Confirm deletion if needed. A dry run removes nothing, so it never asks:
        // prompting there fails outright when stdin is not a terminal.
        if !self.targets.is_empty() && !self.config.skip_confirmation && !self.config.dry_run {
            let confirmation = Confirm::new()
                .with_prompt("Do you want to delete the above?")
                .interact()
                .map_err(|e| CleanError::ConfigError(format!("Confirmation failed: {}", e)))?;

            if !confirmation {
                warn!("Cleaning operation cancelled.");
                return Ok(());
            }
        }

        if !self.config.dry_run {
            self.execute_deletion();
        }

        // Display summary (suppressed in JSON mode)
        if !self.config.dry_run && self.counter > 0 && !self.config.json_mode {
            info!(
                "Deleted {} item(s) totalling {}",
                self.counter,
                format_size(self.size)
            );
        }

        Ok(())
    }

    /// Build globsets for include and exclude patterns, plus individual matchers for stats
    fn build_globsets(&self) -> Result<(GlobSet, Option<GlobSet>, PatternMatchers)> {
        let mut builder = GlobSetBuilder::new();
        let mut matchers = Vec::new();

        for pattern in self.config.patterns.iter() {
            let glob = Glob::new(pattern)?;
            builder.add(glob.clone());
            matchers.push((pattern.clone(), glob.compile_matcher()));
        }
        let include_set = builder.build()?;

        let exclude_set = if !self.config.exclude_patterns.is_empty() {
            let mut builder = GlobSetBuilder::new();
            for pattern in self.config.exclude_patterns.iter() {
                builder.add(Glob::new(pattern)?);
            }
            Some(builder.build()?)
        } else {
            None
        };

        Ok((include_set, exclude_set, matchers))
    }

    /// Collect targets for deletion
    fn collect_targets(
        &mut self,
        path: &Path,
        base_path: &Path,
        include_set: &GlobSet,
        exclude_set: &Option<GlobSet>,
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
            include_set,
            exclude_set,
            matchers,
            progress,
            processed: AtomicU64::new(0),
            collected: Mutex::new(Vec::new()),
        };

        // The root is considered on its own, exempt from protection: pointing rclean
        // at `.git` is a deliberate act, and silently doing nothing there would be
        // its own trap.
        let root_type = fs::symlink_metadata(path)
            .map_err(|e| CleanError::ConfigError(format!("Cannot read '{:?}': {}", path, e)))?
            .file_type();

        if scan.consider(path, None, root_type) {
            parallel_dir_queue(vec![path.to_path_buf()], |dir| scan.visit_dir(dir));
        }

        let Scan {
            progress,
            processed,
            collected,
            ..
        } = scan;

        self.targets = collected.into_inner().unwrap();
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
    fn size_directories(&mut self) {
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

        for (&index, size) in indices.iter().zip(parallel_dir_sizes(&dirs)) {
            self.targets[index].size = size;
        }
    }

    /// Accumulate totals, statistics and per-item output for the collected targets
    fn report_targets(&mut self) {
        // Targets deleted without a prompt are reported by `execute_deletion` instead,
        // so a run does not list the same path twice.
        let announce = !self.deletes_unprompted() && !self.config.json_mode;

        for target in self.targets.iter() {
            self.size += target.size;
            self.counter += 1;

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
                });
            }

            if announce {
                info!("Matched: {:?}", target.path.display());
            }
        }
    }

    /// Execute deletion of collected targets
    fn execute_deletion(&mut self) {
        let targets_to_delete = std::mem::take(&mut self.targets);
        let announce = self.deletes_unprompted() && !self.config.json_mode;

        // No target can sit inside another: the walk stops descending at a matched
        // directory, so a child of one is never collected.
        for target in targets_to_delete.iter() {
            if self.remove_path(&target.path, &target.metadata) && announce {
                info!("Deleted: {:?}", target.path.display());
            }
        }

        // Display error summary if there were failures
        if !self.failed_deletions.is_empty() {
            error!("\n=== Deletion Failures ===");
            for (path, err_msg) in &self.failed_deletions {
                error!("  {:?}: {}", path.display(), err_msg);
            }
            error!("Total failures: {}\n", self.failed_deletions.len());
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

    /// Remove file or directory, returning whether it was removed
    fn remove_path(&mut self, path: &Path, metadata: &Metadata) -> bool {
        let result = if metadata.is_dir() {
            fs::remove_dir_all(path)
        } else if metadata.is_file() || metadata.is_symlink() {
            fs::remove_file(path)
        } else {
            warn!("skipping unknown file type: {:?}", path.display());
            return false;
        };

        if let Err(e) = result {
            let error_msg = format!("{}", e);
            self.failed_deletions.push((path.to_path_buf(), error_msg));
            error!("Failed to remove {:?}: {}", path.display(), e);
            return false;
        }
        true
    }
}
