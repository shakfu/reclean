// --------------------------------------------------------------------
// imports

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use log::{error, info};
use std::fs;
use std::io;
use std::path::Path;
use std::process;

use rclean::constants::{
    get_artifact_dirs, get_default_excludes, get_default_patterns, get_preset_patterns,
    get_protected_dirs, DEFAULT_EXCLUDES, PRESET_NAMES, SETTINGS_FILENAME,
};
use rclean::{discover_config, parse_duration, CleanConfig, CleaningJob, Result};

// --------------------------------------------------------------------
// cli api

/// Program to cleanup non-essential files or directories
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Working Directory [default: .]
    #[arg(short, long)]
    path: Option<String>,

    /// Specify custom glob pattern(s)
    #[arg(short, long)]
    glob: Option<Vec<String>>,

    /// Exclude patterns (files matching these will not be deleted)
    #[arg(short, long)]
    exclude: Option<Vec<String>>,

    /// Configure from config file (searches upward then ~/.config/rclean/ if no path given)
    #[arg(short, long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    configfile: Option<String>,

    /// Write default '.rclean.toml' file
    #[arg(short, long, conflicts_with = "configfile")]
    write_configfile: bool,

    /// Dry-run without actual removal
    #[arg(short, long)]
    dry_run: bool,

    /// Skip confirmation
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

    /// Display statistics by pattern
    #[arg(short = 's', long)]
    stats: bool,

    /// Only remove files older than specified duration (e.g., "30d", "7d", "24h", "3600s")
    #[arg(short = 'o', long)]
    older_than: Option<String>,

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

    /// Use a named preset pattern group (common, python, node, rust, java, c, go, all)
    #[arg(long)]
    preset: Option<Vec<String>>,

    /// Generate shell completions (bash, zsh, fish, elvish, powershell)
    #[arg(long, value_name = "SHELL")]
    completions: Option<Shell>,

    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    format: OutputFormat,

    /// list default glob patterns
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

/// Configure and initialize logging at the given level
fn init_logging(level: simplelog::LevelFilter) {
    let logging_config = simplelog::ConfigBuilder::new()
        .set_level_color(simplelog::Level::Info, Some(simplelog::Color::Green))
        .set_level_color(simplelog::Level::Trace, Some(simplelog::Color::Magenta))
        .build();

    simplelog::TermLogger::init(
        level,
        logging_config,
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )
    .expect("could not initialize logging");
}

/// Generate default config file: '.rclean.toml'
fn write_configfile(config: &CleanConfig) -> Result<()> {
    let toml = toml::to_string(config).map_err(|e| {
        rclean::CleanError::ConfigError(format!("Failed to serialize config: {}", e))
    })?;

    let cfg_out = Path::new(SETTINGS_FILENAME);
    if !cfg_out.exists() {
        info!("generating default '{SETTINGS_FILENAME}' file");
        fs::write(cfg_out, toml)?;
        Ok(())
    } else {
        Err(rclean::CleanError::ConfigError(format!(
            "Cannot overwrite existing '{SETTINGS_FILENAME}' file"
        )))
    }
}

/// Resolve the patterns named on the command line, combining `--glob` with
/// `--preset`. `None` when neither flag is given.
fn resolve_patterns(args: &Args) -> Result<Option<Vec<String>>> {
    if args.glob.is_none() && args.preset.is_none() {
        return Ok(None);
    }

    let mut patterns = args.glob.clone().unwrap_or_default();
    for name in args.preset.iter().flatten() {
        match get_preset_patterns(name) {
            Some(p) => patterns.extend(p),
            None => {
                return Err(rclean::CleanError::ConfigError(format!(
                    "Unknown preset '{}'. Available: {}",
                    name,
                    PRESET_NAMES.join(", ")
                )))
            }
        }
    }
    Ok(Some(patterns))
}

/// Run one job, report it in the requested format, and fail on failed deletions.
fn run_job(config: CleanConfig) -> Result<()> {
    let json_mode = config.json_mode;
    let mut job = CleaningJob::new(config);
    job.run()?;

    if json_mode {
        let json = job.to_json().map_err(|e| {
            rclean::CleanError::ConfigError(format!("Failed to serialize JSON: {}", e))
        })?;
        println!("{}", json);
    }

    if job.has_failures() {
        return Err(rclean::CleanError::ConfigError(format!(
            "{} deletion(s) failed",
            job.failed_deletions.len()
        )));
    }

    Ok(())
}

/// Load config from file, then apply CLI overrides.
/// If no explicit path is given, searches upward for `.rclean.toml`
/// then falls back to `~/.config/rclean/config.toml`.
fn run_job_from_configfile(config_path: Option<String>, args: &Args) -> Result<()> {
    let resolved_path = if let Some(ref explicit) = config_path.filter(|s| !s.is_empty()) {
        // Explicit path provided -- use it directly
        let p = Path::new(explicit);
        if !p.exists() {
            return Err(rclean::CleanError::ConfigError(format!(
                "Settings file '{}' not found",
                explicit
            )));
        }
        p.to_path_buf()
    } else {
        // No explicit path -- discover config
        let cwd = std::env::current_dir().map_err(|e| {
            rclean::CleanError::ConfigError(format!("Cannot get current directory: {}", e))
        })?;
        discover_config(&cwd).ok_or_else(|| {
            rclean::CleanError::ConfigError(format!(
                "No '{}' found in directory tree or global config (~/.config/rclean/config.toml)",
                SETTINGS_FILENAME
            ))
        })?
    };

    let config_file = resolved_path.to_str().unwrap_or(SETTINGS_FILENAME);

    info!("using settings file: {config_file:?}");
    let contents = fs::read_to_string(config_file)?;
    let mut config: CleanConfig = toml::from_str(&contents).map_err(|e| {
        rclean::CleanError::ConfigError(format!("Cannot deserialize from .toml: {}", e))
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
    if let Some(ref path) = args.path {
        config.path = path.clone();
    }
    if let Some(patterns) = resolve_patterns(args)? {
        config.patterns = patterns;
    }
    config.json_mode = args.format == OutputFormat::Json;

    run_job(config)
}

/// Execute the CLI logic, returning Result for clean error propagation
fn run(args: Args) -> Result<()> {
    // Generate shell completions and exit
    if let Some(shell) = args.completions {
        let mut cmd = Args::command();
        generate(shell, &mut cmd, "rclean", &mut io::stdout());
        return Ok(());
    }

    if args.list {
        if let Some(ref presets) = args.preset {
            for name in presets {
                if let Some(patterns) = get_preset_patterns(name) {
                    info!("{} preset patterns: {:?}", name, patterns);
                } else {
                    return Err(rclean::CleanError::ConfigError(format!(
                        "Unknown preset '{}'. Available: {}",
                        name,
                        PRESET_NAMES.join(", ")
                    )));
                }
            }
        } else {
            info!("default patterns: {:?}", get_default_patterns());
            info!("available presets: {}", PRESET_NAMES.join(", "));
            info!("protected directories: {:?}", get_protected_dirs());
            info!("default excludes: {:?}", get_default_excludes());
            info!("build artifact directories: {:?}", get_artifact_dirs());
        }
        return Ok(());
    }

    if args.configfile.is_some() {
        return run_job_from_configfile(args.configfile.clone(), &args);
    }

    // Parse duration if provided
    let older_than_secs = match args.older_than {
        Some(ref duration_str) => Some(parse_duration(duration_str)?),
        None => None,
    };

    // Resolve patterns: --glob and --preset combine, otherwise the defaults
    let patterns = resolve_patterns(&args)?.unwrap_or_else(get_default_patterns);

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
        .protected_dirs(if args.no_protect {
            Vec::new()
        } else {
            get_protected_dirs()
        })
        .build();

    if args.write_configfile {
        return write_configfile(&config);
    }

    run_job(config)
}

/// main function
fn main() {
    let args = Args::parse();

    // Determine log level from flags
    let log_level = if args.quiet {
        simplelog::LevelFilter::Error
    } else if args.verbose {
        simplelog::LevelFilter::Debug
    } else {
        simplelog::LevelFilter::Info
    };
    init_logging(log_level);

    if let Err(e) = run(args) {
        error!("Error: {}", e);
        process::exit(1);
    }
}
