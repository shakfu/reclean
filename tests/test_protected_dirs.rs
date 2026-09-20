use rclean::constants::PROTECTED_DIRS;
use rclean::{CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

/// A tree with a .pyc file inside each protected directory, and one in `src`
fn create_protected_structure() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    for dir in [".git/objects", ".config/lib", ".ssh", "src"] {
        fs::create_dir_all(base.join(dir)).unwrap();
        fs::write(base.join(dir).join("cached.pyc"), "payload").unwrap();
    }

    // A __pycache__ nested below a protected directory, not just at its root
    let config_cache = base.join(".config").join("lib").join("__pycache__");
    fs::create_dir_all(&config_cache).unwrap();
    fs::write(config_cache.join("mod.pyc"), "payload").unwrap();

    let src_cache = base.join("src").join("__pycache__");
    fs::create_dir(&src_cache).unwrap();
    fs::write(src_cache.join("mod.pyc"), "payload").unwrap();

    temp_dir
}

#[test]
fn test_protected_dirs_are_not_entered() {
    let temp_dir = create_protected_structure();
    let base = temp_dir.path();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string(), "**/__pycache__".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Only what lives under src is matched: the directory, and the loose file
    assert_eq!(job.counter, 2);

    assert!(base.join(".git/objects/cached.pyc").exists());
    assert!(base.join(".config/lib/cached.pyc").exists());
    assert!(base.join(".config/lib/__pycache__").exists());
    assert!(base.join(".ssh/cached.pyc").exists());

    assert!(!base.join("src/cached.pyc").exists());
    assert!(!base.join("src/__pycache__").exists());
}

#[test]
fn test_protected_dir_itself_is_never_matched() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    for name in PROTECTED_DIRS {
        fs::create_dir(base.join(name)).unwrap();
        fs::write(base.join(name).join("file.txt"), "payload").unwrap();
    }

    // A pattern aimed straight at the protected names
    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(PROTECTED_DIRS.iter().map(|d| format!("**/{}", d)).collect())
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 0);
    for name in PROTECTED_DIRS {
        assert!(base.join(name).exists(), "{} was removed", name);
    }
}

#[test]
fn test_protected_name_as_file_is_not_matched() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let module = base.join("submodule");
    fs::create_dir(&module).unwrap();
    // A submodule marks its git directory with a file, not a directory
    fs::write(module.join(".git"), "gitdir: ../.git/modules/submodule").unwrap();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/.git".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 0);
    assert!(module.join(".git").exists());
}

#[test]
fn test_protection_can_be_disabled() {
    let temp_dir = create_protected_structure();
    let base = temp_dir.path();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .protected_dirs(Vec::new())
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // .git, .config (twice), .ssh, src (twice) -- the six the comparison reported
    assert_eq!(job.counter, 6);
}

#[test]
fn test_protection_list_is_configurable() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    for dir in ["keepme", "cleanme"] {
        fs::create_dir(base.join(dir)).unwrap();
        fs::write(base.join(dir).join("a.pyc"), "payload").unwrap();
    }

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .protected_dirs(vec!["keepme".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 1);
    assert!(base.join("keepme/a.pyc").exists());
    assert!(!base.join("cleanme/a.pyc").exists());
}

#[test]
fn test_protection_exempts_an_explicitly_named_root() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let git = base.join(".git");
    fs::create_dir(&git).unwrap();
    fs::write(git.join("stale.pyc"), "payload").unwrap();

    // Pointing at a protected directory is deliberate, so the walk still enters it
    let config = CleanConfig::builder()
        .path(git.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 1);
}

#[test]
fn test_config_without_protected_dirs_field_gets_defaults() {
    // A .rclean.toml written before protection existed must still be protected
    let toml = r#"
path = "."
patterns = ["**/*.pyc"]
dry_run = false
skip_confirmation = false
include_symlinks = false
remove_broken_symlinks = false
"#;
    let config: CleanConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.protected_dirs, PROTECTED_DIRS);
}
