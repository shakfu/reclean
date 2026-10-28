//! Patterns match the path below the root, and the root is never a target.

use reclean::{CleanConfig, CleaningJob};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn run(root: &Path, patterns: &[&str], excludes: &[&str]) -> CleaningJob {
    let config = CleanConfig::builder()
        .path(root.to_str().unwrap())
        .patterns(patterns.iter().map(|p| p.to_string()).collect())
        .exclude_patterns(excludes.iter().map(|p| p.to_string()).collect())
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();
    job
}

#[test]
fn test_pattern_matching_the_root_path_keeps_the_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().join("cache-tests");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("main.rs"), "x").unwrap();

    // `*cache*` matched the root's own text, which claimed the whole tree
    let job = run(&root, &["*cache*"], &[]);

    assert_eq!(job.counter, 0);
    assert!(root.join("src/main.rs").exists());
}

#[test]
fn test_root_named_like_a_pattern_is_not_a_target() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().join("__pycache__");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("keep.txt"), "x").unwrap();

    let job = run(&root, &["**/__pycache__"], &[]);

    assert_eq!(job.counter, 0);
    assert!(root.join("keep.txt").exists());
}

#[test]
fn test_match_everything_spares_the_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::create_dir(root.join("a")).unwrap();
    fs::write(root.join("a").join("x"), "x").unwrap();
    fs::write(root.join("b"), "x").unwrap();

    let job = run(root, &["**"], &[]);

    // `a` is claimed whole, so `a/x` is not a second target
    assert_eq!(job.counter, 2);
    assert!(root.exists());
}

#[test]
fn test_relative_patterns_match_below_the_root() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("a.c"), "x").unwrap();
    fs::write(root.join("a.c"), "x").unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs").join("keep.pyc"), "x").unwrap();
    fs::write(root.join("docs").join("drop.pyc"), "x").unwrap();

    let job = run(root, &["src/*.c", "**/*.pyc"], &["docs/keep.pyc"]);

    assert_eq!(job.counter, 2);
    assert!(!root.join("src/a.c").exists());
    assert!(root.join("a.c").exists());
    assert!(root.join("docs/keep.pyc").exists());
    assert!(!root.join("docs/drop.pyc").exists());
}

#[test]
fn test_rerun_does_not_accumulate() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();
    fs::write(root.join("a.pyc"), "x").unwrap();

    let config = CleanConfig::builder()
        .path(root.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .stats_mode(true)
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();
    job.run().unwrap();

    assert_eq!(job.counter, 1);
    assert_eq!(job.size, 1);
    assert_eq!(job.stats.get("**/*.pyc"), Some(&(1, 1)));
}
