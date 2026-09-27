use reclean::constants::DEFAULT_EXCLUDES;
use reclean::{CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

/// A virtualenv holding one `__pycache__`, beside one in `src`
fn create_venv_structure(name: &str) -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    for dir in [base.join(name).join("lib"), base.join("src")] {
        let cache = dir.join("__pycache__");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("mod.pyc"), "payload").unwrap();
    }

    temp_dir
}

fn job_for(path: &std::path::Path) -> CleanConfig {
    CleanConfig::builder()
        .path(path.to_str().unwrap())
        .patterns(vec!["**/__pycache__".to_string()])
        .skip_confirmation(true)
        .build()
}

#[test]
fn test_virtualenvs_are_excluded_by_default() {
    for name in [".venv", "venv"] {
        let temp_dir = create_venv_structure(name);
        let base = temp_dir.path();

        let mut job = CleaningJob::new(job_for(base));
        job.run().unwrap();

        assert_eq!(job.counter, 1, "{} was not excluded", name);
        assert!(base.join(name).join("lib/__pycache__").exists());
        assert!(!base.join("src/__pycache__").exists());
    }
}

#[test]
fn test_an_excluded_directory_is_not_entered() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let nested = base.join("build").join("deep").join("__pycache__");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("mod.pyc"), "payload").unwrap();

    // The exclude names the directory, not the items inside it
    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/__pycache__".to_string(), "**/*.pyc".to_string()])
        .exclude_patterns(vec!["**/build".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 0);
    assert!(nested.join("mod.pyc").exists());
}

#[test]
fn test_excluded_root_is_still_cleaned() {
    let temp_dir = create_venv_structure(".venv");
    let venv = temp_dir.path().join(".venv");

    // Naming a virtualenv on --path is deliberate, so the default exclude yields
    let mut job = CleaningJob::new(job_for(&venv));
    job.run().unwrap();

    assert_eq!(job.counter, 1);
    assert!(!venv.join("lib/__pycache__").exists());
}

#[test]
fn test_excludes_are_replaceable() {
    let temp_dir = create_venv_structure(".venv");
    let base = temp_dir.path();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/__pycache__".to_string()])
        .exclude_patterns(Vec::new())
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 2);
    assert!(!base.join(".venv/lib/__pycache__").exists());
}

#[test]
fn test_config_without_exclude_patterns_field_gets_defaults() {
    // A .reclean.toml written before the default excludes existed still gets them
    let toml = r#"
path = "."
patterns = ["**/*.pyc"]
dry_run = false
skip_confirmation = false
include_symlinks = false
remove_broken_symlinks = false
"#;
    let config: CleanConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.exclude_patterns, DEFAULT_EXCLUDES);
    // Named outright, so an emptied DEFAULT_EXCLUDES cannot satisfy the check above
    assert_eq!(config.exclude_patterns, vec!["**/.venv", "**/venv"]);
}
