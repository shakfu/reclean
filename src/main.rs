// --------------------------------------------------------------------
// imports

use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use clap_complete::{generate, Shell};
use log::{error, info};
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process;

use reclean::constants::{
    get_artifact_dirs, get_default_excludes, get_default_patterns, get_dependency_dirs,
    get_protected_dirs, DEFAULT_EXCLUDES, SETTINGS_FILENAME,
};
use reclean::{discover_config, parse_duration, parse_size, CleanConfig, CleaningJob, Result};

// --------------------------------------------------------------------
// cli api

/// Program to cleanup non-essential files or directories
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Working directory [default: .]
    #[arg(short, long)]
    path: Option<String>,

    /// Include glob pattern(s), matched below the working directory
    #[arg(short, long)]
    glob: Option<Vec<String>>,

    /// Exclude glob pattern(s); an excluded directory is not entered
    #[arg(short, long)]
    exclude: Option<Vec<String>>,

    /// Load config (searches upward, then ~/.config/reclean/)
    #[arg(short, long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    configfile: Option<String>,

    /// Write default '.reclean.toml' file
    #[arg(short, long, conflicts_with = "configfile")]
    write_configfile: bool,

    /// Preview deletions without removing
    #[arg(short, long)]
    dry_run: bool,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    skip_confirmation: bool,

    /// Include matched symlinks for removal
    #[arg(short, long)]
    include_symlinks: bool,

    /// Remove broken symlinks
    #[arg(short, long)]
    remove_broken_symlinks: bool,

    /// Also match build output at the top level of a project (build, target, dist, ...)
    #[arg(short = 'B', long)]
    build_artifacts: bool,

    /// Also match dependency trees beside their lock file (node_modules, .venv, vendor)
    #[arg(short = 'D', long)]
    dependencies: bool,

    /// Display statistics by pattern
    #[arg(short = 's', long)]
    stats: bool,

    /// Only remove targets older than this duration (e.g., "30d", "7d", "24h")
    #[arg(short = 'o', long, value_name = "DURATION")]
    older_than: Option<String>,

    /// Only remove targets of at least this size (e.g., "100M", "1.5G", "4096")
    #[arg(long, value_name = "SIZE")]
    larger_than: Option<String>,

    /// Show progress bar during scanning
    #[arg(short = 'P', long)]
    progress: bool,

    /// Match inside protected (.git, .ssh, ...) and excluded-by-default (.venv) directories
    #[arg(long)]
    no_protect: bool,

    /// Increase verbosity (debug-level logging)
    #[arg(short, long)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short = 'q', long)]
    quiet: bool,

    /// Generate shell completions (bash, zsh, fish, elvish, powershell)
    #[arg(long, value_name = "SHELL")]
    completions: Option<Shell>,

    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    format: OutputFormat,

    /// When to colour stderr output (auto: on a terminal, unless NO_COLOR is set)
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: clap::ColorChoice,

    /// List default patterns, excludes, protected, artifact and dependency names
    #[arg(short, long)]
    list: bool,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
enum OutputFormat {
    Text,
    Json,
}

// --------------------------------------------------------------------
// main function

/// Writes this crate's log records to stderr. Info lines have no prefix and
/// are green, except `Matched:`/`Deleted:` lines, which colour only the label;
/// other levels carry a coloured `warning:`-style prefix. Colour applies only
/// when `colour` is set.
struct StderrLogger {
    colour: bool,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Dependencies such as globset log at debug, which `-v` should not show
        metadata.level() <= log::max_level() && metadata.target().starts_with("reclean")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let (prefix, sgr) = match record.level() {
            log::Level::Info => ("", "32"),
            log::Level::Error => ("error: ", "1;31"),
            log::Level::Warn => ("warning: ", "1;33"),
            log::Level::Debug => ("debug: ", "36"),
            log::Level::Trace => ("trace: ", "35"),
        };
        // `eprintln!` panics when stderr is closed, which would abort a run
        // mid-report; a lost log line is the lesser failure.
        let mut err = io::stderr().lock();
        // Per-target info lines colour only their label; other info lines are whole-line
        let _ = if self.colour && prefix.is_empty() {
            let msg = record.args().to_string();
            match [("Matched", "32"), ("Deleted", "1;37")]
                .into_iter()
                .find_map(|(label, sgr)| {
                    Some((label, sgr, msg.strip_prefix(label)?.strip_prefix(':')?))
                }) {
                Some((label, sgr, rest)) => writeln!(err, "\x1b[{sgr}m{label}\x1b[0m:{rest}"),
                None => writeln!(err, "\x1b[{sgr}m{msg}\x1b[0m"),
            }
        } else if self.colour {
            writeln!(err, "\x1b[{sgr}m{prefix}\x1b[0m{}", record.args())
        } else {
            writeln!(err, "{prefix}{}", record.args())
        };
    }

    fn flush(&self) {}
}

/// Resolve `--color`. `auto` colours only a terminal, and never when `NO_COLOR`
/// is set to a non-empty value; `always` overrides `NO_COLOR`.
fn colour_enabled(choice: clap::ColorChoice) -> bool {
    match choice {
        clap::ColorChoice::Always => true,
        clap::ColorChoice::Never => false,
        clap::ColorChoice::Auto => {
            io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
        }
    }
}

/// Find `--color` before parsing, so clap's own help and errors obey it.
/// The last valid value wins; an invalid one is left for clap to reject.
fn prescan_color(argv: &[OsString]) -> clap::ColorChoice {
    let mut choice = clap::ColorChoice::Auto;
    let mut iter = argv.iter().skip(1);
    while let Some(arg) = iter.next() {
        let value = match arg.to_str() {
            Some("--") => break,
            Some("--color") => iter.next().and_then(|v| v.to_str()),
            Some(a) => a.strip_prefix("--color="),
            None => None,
        };
        if let Some(c) = value.and_then(|v| clap::ColorChoice::from_str(v, false).ok()) {
            choice = c;
        }
    }
    choice
}

/// Route log records to stderr at the given level.
fn init_logging(level: log::LevelFilter, colour: bool) {
    let logger = Box::leak(Box::new(StderrLogger { colour }));
    log::set_logger(logger).expect("could not initialize logging");
    log::set_max_level(level);
}

/// Generate default config file: '.reclean.toml'
fn write_configfile(config: &CleanConfig) -> Result<()> {
    let toml = toml::to_string(config).map_err(|e| {
        reclean::CleanError::ConfigError(format!("Failed to serialize config: {}", e))
    })?;

    let cfg_out = Path::new(SETTINGS_FILENAME);
    if !cfg_out.exists() {
        info!("generating default '{SETTINGS_FILENAME}' file");
        fs::write(cfg_out, toml)?;
        Ok(())
    } else {
        Err(reclean::CleanError::ConfigError(format!(
            "Cannot overwrite existing '{SETTINGS_FILENAME}' file"
        )))
    }
}

/// Exit status for a run that completed but could not read part of the tree
const EXIT_WARNINGS: i32 = 3;

/// Run one job, report it in the requested format, and return its exit status.
fn run_job(config: CleanConfig) -> Result<i32> {
    let json_mode = config.json_mode;
    let mut job = CleaningJob::new(config);
    job.run()?;

    if json_mode {
        let json = job.to_json().map_err(|e| {
            reclean::CleanError::ConfigError(format!("Failed to serialize JSON: {}", e))
        })?;
        println!("{}", json);
    }

    if job.has_failures() {
        return Ok(1);
    }
    if job.has_warnings() {
        return Ok(EXIT_WARNINGS);
    }

    Ok(0)
}

/// Load config from file, then apply CLI overrides.
/// If no explicit path is given, searches upward for `.reclean.toml`
/// then falls back to `~/.config/reclean/config.toml`.
fn run_job_from_configfile(config_path: Option<String>, args: &Args) -> Result<i32> {
    let resolved_path = if let Some(ref explicit) = config_path.filter(|s| !s.is_empty()) {
        // Explicit path provided -- use it directly
        let p = Path::new(explicit);
        if !p.exists() {
            return Err(reclean::CleanError::ConfigError(format!(
                "Settings file '{}' not found",
                explicit
            )));
        }
        p.to_path_buf()
    } else {
        // No explicit path -- discover config
        let cwd = std::env::current_dir().map_err(|e| {
            reclean::CleanError::ConfigError(format!("Cannot get current directory: {}", e))
        })?;
        discover_config(&cwd).ok_or_else(|| {
            reclean::CleanError::ConfigError(format!(
                "No '{}' found in directory tree or global config ({:?})",
                SETTINGS_FILENAME,
                reclean::global_config_locations()
            ))
        })?
    };

    info!("using settings file: {resolved_path:?}");
    let contents = fs::read_to_string(&resolved_path).map_err(|e| {
        reclean::CleanError::ConfigError(format!("Cannot read {:?}: {}", resolved_path, e))
    })?;
    let mut config: CleanConfig = toml::from_str(&contents).map_err(|e| {
        reclean::CleanError::ConfigError(format!("Cannot deserialize from .toml: {}", e))
    })?;

    // CLI flags override config file values when explicitly set
    if args.dry_run {
        config.dry_run = true;
    }
    if args.skip_confirmation {
        config.skip_confirmation = true;
    }
    if args.include_symlinks {
        config.include_symlinks = true;
    }
    if args.remove_broken_symlinks {
        config.remove_broken_symlinks = true;
    }
    if args.stats {
        config.stats_mode = true;
    }
    if args.progress {
        config.show_progress = true;
    }
    if args.build_artifacts {
        config.build_artifacts = true;
    }
    if args.dependencies {
        config.dependencies = true;
    }
    if args.no_protect {
        config.protected_dirs.clear();
        // Only the built-in excludes go: patterns the config file names are the
        // user's own, and this flag is about the defaults.
        config
            .exclude_patterns
            .retain(|p| !DEFAULT_EXCLUDES.contains(&p.as_str()));
    }
    if let Some(ref excludes) = args.exclude {
        config.exclude_patterns.extend(excludes.clone());
    }
    if let Some(ref duration_str) = args.older_than {
        config.older_than_secs = Some(parse_duration(duration_str)?);
    }
    if let Some(ref size) = args.larger_than {
        config.larger_than_bytes = Some(parse_size(size)?);
    }
    if let Some(ref path) = args.path {
        config.path = path.clone();
    }
    if let Some(ref patterns) = args.glob {
        config.patterns = patterns.clone();
    }
    config.json_mode = args.format == OutputFormat::Json;
    config.config_file = Some(resolved_path.clone());

    run_job(config)
}

/// Execute the CLI logic, returning the exit status of a completed run
fn run(args: Args) -> Result<i32> {
    // Generate shell completions and exit
    if let Some(shell) = args.completions {
        let mut cmd = Args::command();
        generate(shell, &mut cmd, "reclean", &mut io::stdout());
        return Ok(0);
    }

    // Printed to stdout, one name per line, so the lists can be piped
    if args.list {
        for (heading, names) in [
            ("default patterns", get_default_patterns()),
            ("protected directories", get_protected_dirs()),
            ("default excludes", get_default_excludes()),
            ("build artifact directories", get_artifact_dirs()),
            ("dependency directories", get_dependency_dirs()),
        ] {
            println!("{heading}:");
            for name in names {
                println!("  {name}");
            }
        }
        return Ok(0);
    }

    if args.configfile.is_some() {
        return run_job_from_configfile(args.configfile.clone(), &args);
    }

    // Parse duration if provided
    let older_than_secs = match args.older_than {
        Some(ref duration_str) => Some(parse_duration(duration_str)?),
        None => None,
    };
    let larger_than_bytes = match args.larger_than {
        Some(ref size) => Some(parse_size(size)?),
        None => None,
    };

    // `--glob` replaces the defaults
    let patterns = args.glob.clone().unwrap_or_else(get_default_patterns);

    // The defaults carry `--exclude` unless the run opts out of them entirely
    let mut exclude_patterns = if args.no_protect {
        Vec::new()
    } else {
        get_default_excludes()
    };
    exclude_patterns.extend(args.exclude.clone().unwrap_or_default());

    let config = CleanConfig::builder()
        .path(args.path.clone().unwrap_or_else(|| ".".to_string()))
        .patterns(patterns)
        .exclude_patterns(exclude_patterns)
        .dry_run(args.dry_run)
        .skip_confirmation(args.skip_confirmation)
        .include_symlinks(args.include_symlinks)
        .remove_broken_symlinks(args.remove_broken_symlinks)
        .stats_mode(args.stats)
        .older_than_secs(older_than_secs)
        .show_progress(args.progress)
        .json_mode(args.format == OutputFormat::Json)
        .build_artifacts(args.build_artifacts)
        .dependencies(args.dependencies)
        .larger_than_bytes(larger_than_bytes)
        .protected_dirs(if args.no_protect {
            Vec::new()
        } else {
            get_protected_dirs()
        })
        .build();

    if args.write_configfile {
        return write_configfile(&config).map(|()| 0);
    }

    run_job(config)
}

/// main function
fn main() {
    let argv: Vec<OsString> = std::env::args_os().collect();
    let matches = Args::command()
        .color(prescan_color(&argv))
        .get_matches_from(argv);
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());

    // Determine log level from flags
    let log_level = if args.quiet {
        log::LevelFilter::Error
    } else if args.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    let colour = colour_enabled(args.color);
    // The progress spinner and confirmation prompt colour through `console`,
    // which ignores `NO_COLOR` unless told otherwise
    dialoguer::console::set_colors_enabled_stderr(colour);
    init_logging(log_level, colour);

    match run(args) {
        Ok(0) => {}
        Ok(code) => {
            // `process::exit` runs no destructors, so flush what was printed
            let _ = io::stdout().flush();
            process::exit(code);
        }
        Err(e) => {
            error!("{}", e);
            // Usage and configuration errors are 2, as clap's own are
            let code = match e {
                reclean::CleanError::ConfigError(_) | reclean::CleanError::GlobError(_) => 2,
                _ => 1,
            };
            process::exit(code);
        }
    }
}
