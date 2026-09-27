use reclean::{CleanConfig, CleaningJob};
use std::fs;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Helper function to create a temporary directory structure for testing
fn create_test_structure() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create test files
    fs::write(base.join("test.txt"), "test content").unwrap();
    fs::write(base.join("test.pyc"), "compiled python").unwrap();
    fs::write(base.join("important.log"), "keep this").unwrap();

    // Create __pycache__ directory
    let pycache = base.join("__pycache__");
    fs::create_dir(&pycache).unwrap();
    fs::write(pycache.join("module.pyc"), "cached").unwrap();
    fs::write(pycache.join("another.pyc"), "cached2").unwrap();

    // Create nested directory structure
    let subdir = base.join("subdir");
    fs::create_dir(&subdir).unwrap();
    fs::write(subdir.join("test.pyc"), "nested pyc").unwrap();

    temp_dir
}

#[test]
fn test_dry_run_does_not_delete() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Files should still exist in dry-run mode
    assert!(temp_dir.path().join("test.pyc").exists());
    assert!(temp_dir.path().join("subdir/test.pyc").exists());
}

#[test]
fn test_actual_file_deletion() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // .pyc files should be deleted
    assert!(!temp_dir.path().join("test.pyc").exists());
    assert!(!temp_dir.path().join("subdir/test.pyc").exists());

    // Other files should remain
    assert!(temp_dir.path().join("test.txt").exists());
    assert!(temp_dir.path().join("important.log").exists());
}

#[test]
fn test_directory_deletion() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/__pycache__".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // __pycache__ directory should be deleted entirely
    assert!(!temp_dir.path().join("__pycache__").exists());

    // Other files should remain
    assert!(temp_dir.path().join("test.txt").exists());
    assert!(temp_dir.path().join("test.pyc").exists());
}

#[test]
fn test_multiple_patterns() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string(), "**/__pycache__".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Both .pyc files and __pycache__ directory should be deleted
    assert!(!temp_dir.path().join("test.pyc").exists());
    assert!(!temp_dir.path().join("__pycache__").exists());
    assert!(!temp_dir.path().join("subdir/test.pyc").exists());

    // Other files should remain
    assert!(temp_dir.path().join("test.txt").exists());
    assert!(temp_dir.path().join("important.log").exists());
}

#[test]
fn test_broken_symlink_removal() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create a file and a symlink to it
    let target = base.join("target.txt");
    fs::write(&target, "content").unwrap();
    let link = base.join("link.txt");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();

    // Remove the target to break the symlink
    fs::remove_file(&target).unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .remove_broken_symlinks(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Broken symlink should be removed
    assert!(!link.exists());
}

#[test]
fn test_invalid_pattern_returns_error() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["[invalid".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);

    let result = job.run();
    assert!(result.is_err());
}

#[test]
fn test_size_calculation() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Size should be greater than 0 since we have .pyc files
    assert!(job.size > 0);
    // Counter should match the number of .pyc files created
    // test.pyc (root), __pycache__/module.pyc, __pycache__/another.pyc, subdir/test.pyc
    assert_eq!(job.counter, 4);
}

#[test]
fn test_path_traversal_protection() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create a file we should NOT be able to delete
    let outside_file = base.parent().unwrap().join("outside.txt");
    fs::write(&outside_file, "protected").unwrap();

    // Try to use a pattern that would match outside the base directory
    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["../../*.txt".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);

    // Run should succeed but not delete files outside base directory
    job.run().unwrap();

    // File outside should still exist (protected by canonicalization check)
    assert!(outside_file.exists());

    // Cleanup
    fs::remove_file(outside_file).unwrap();
}

#[test]
fn test_exclude_patterns() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .exclude_patterns(vec!["**/subdir/*.pyc".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // .pyc files in root should be deleted
    assert!(!temp_dir.path().join("test.pyc").exists());

    // .pyc files in subdir should be excluded
    assert!(temp_dir.path().join("subdir/test.pyc").exists());
}

#[test]
fn test_stats_mode() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string(), "**/__pycache__".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .stats_mode(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Statistics should be populated
    assert!(!job.stats.is_empty());

    // Should have entries for both patterns
    assert!(job.stats.contains_key("**/*.pyc") || job.stats.contains_key("**/__pycache__"));

    // Total count should match
    let total_count: usize = job.stats.values().map(|(count, _)| count).sum();
    assert_eq!(total_count, job.counter);
}

#[test]
fn test_older_than_skips_recent_files() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create a file (will have current timestamp)
    fs::write(base.join("recent.pyc"), "recent content").unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .older_than_secs(Some(3600))
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // File was just created, so it should be skipped (too recent)
    assert_eq!(job.counter, 0);
}

#[test]
fn test_older_than_matches_old_files() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create a file and backdate its modification time
    let old_file = base.join("old.pyc");
    fs::write(&old_file, "old content").unwrap();

    // Set modification time to 2 hours ago
    let two_hours_ago = SystemTime::now() - Duration::from_secs(7200);
    filetime::set_file_mtime(
        &old_file,
        filetime::FileTime::from_system_time(two_hours_ago),
    )
    .unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .older_than_secs(Some(3600))
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // File is 2 hours old, threshold is 1 hour, so it should be matched
    assert_eq!(job.counter, 1);
}

#[test]
fn test_no_failures_when_dir_and_children_both_match() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create nested __pycache__ dirs with .pyc files
    let cache1 = base.join("__pycache__");
    fs::create_dir(&cache1).unwrap();
    fs::write(cache1.join("a.cpython-311.pyc"), "cached").unwrap();
    fs::write(cache1.join("b.cpython-311.pyc"), "cached").unwrap();

    let sub = base.join("pkg");
    fs::create_dir(&sub).unwrap();
    let cache2 = sub.join("__pycache__");
    fs::create_dir(&cache2).unwrap();
    fs::write(cache2.join("c.cpython-311.pyc"), "cached").unwrap();

    let base_path = base.to_str().unwrap().to_string();

    // Patterns that match both the directories AND the files inside them
    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/__pycache__".to_string(), "**/*.pyc".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Everything should be gone
    assert!(!cache1.exists());
    assert!(!cache2.exists());

    // No failures -- child files should be skipped, not produce ENOENT errors
    assert!(!job.has_failures());
}

#[test]
fn test_matched_directory_counted_once() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let cache = base.join("__pycache__");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("a.pyc"), vec![b'x'; 100]).unwrap();
    fs::write(cache.join("b.pyc"), vec![b'x'; 200]).unwrap();

    let base_path = base.to_str().unwrap().to_string();

    // Both patterns match: the directory, and the files it holds
    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/__pycache__".to_string(), "**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // The directory is claimed whole, so its contents are not counted again
    assert_eq!(job.counter, 1);
    assert_eq!(job.size, 300);
}

#[test]
fn test_dry_run_does_not_prompt() {
    let temp_dir = create_test_structure();
    let base_path = temp_dir.path().to_str().unwrap().to_string();

    // No skip_confirmation: a dry run must still complete, since the test harness
    // has no terminal on stdin and prompting there fails
    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .build();
    let mut job = CleaningJob::new(config);

    job.run().unwrap();
    assert_eq!(job.counter, 4);
    assert!(temp_dir.path().join("test.pyc").exists());
}

#[test]
fn test_directory_size_includes_nested_contents() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let cache = base.join("__pycache__");
    fs::create_dir_all(cache.join("a").join("b")).unwrap();
    fs::write(cache.join("top.bin"), vec![b'x'; 10]).unwrap();
    fs::write(cache.join("a").join("mid.bin"), vec![b'x'; 20]).unwrap();
    fs::write(cache.join("a").join("b").join("deep.bin"), vec![b'x'; 30]).unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/__pycache__".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.counter, 1);
    assert_eq!(job.size, 60);
}

#[cfg(unix)]
#[test]
fn test_directory_size_does_not_follow_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let outside = base.join("outside");
    fs::create_dir(&outside).unwrap();
    let big = outside.join("big.bin");
    fs::write(&big, vec![b'x'; 100_000]).unwrap();

    let cache = base.join("__pycache__");
    fs::create_dir(&cache).unwrap();
    std::os::unix::fs::symlink(&big, cache.join("link.bin")).unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/__pycache__".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // The link is counted at its own size; removing the directory never touches
    // the file it points at
    assert_eq!(job.counter, 1);
    assert!(
        job.size < 100_000,
        "symlink target was counted: {}",
        job.size
    );
    assert!(big.exists());
}

#[test]
fn test_nested_matching_directories_counted_once() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    let outer = base.join("node_modules");
    let inner = outer.join("pkg").join("node_modules");
    fs::create_dir_all(&inner).unwrap();
    fs::write(inner.join("f.bin"), vec![b'x'; 50]).unwrap();

    let base_path = base.to_str().unwrap().to_string();

    let config = CleanConfig::builder()
        .path(base_path)
        .patterns(vec!["**/node_modules".to_string()])
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // Only the outer directory is a target; the inner one goes with it
    assert_eq!(job.counter, 1);
    assert_eq!(job.size, 50);
    assert!(!outer.exists());
    assert!(!job.has_failures());
}

/// Set the mtime of `path` to `secs` seconds ago
fn backdate(path: &std::path::Path, secs: u64) {
    let then = SystemTime::now() - Duration::from_secs(secs);
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(then)).unwrap();
}

fn run_older_than(base: &std::path::Path, secs: u64) -> CleaningJob {
    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/__pycache__".to_string(), "**/*.pyc".to_string()])
        .skip_confirmation(true)
        .older_than_secs(Some(secs))
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();
    job
}

#[test]
fn test_older_than_keeps_old_directory_with_recent_contents() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let cache = base.join("__pycache__");
    fs::create_dir_all(cache.join("sub")).unwrap();
    fs::write(cache.join("old.pyc"), "old").unwrap();
    fs::write(cache.join("sub").join("new.pyc"), "new").unwrap();
    backdate(&cache.join("old.pyc"), 7200);
    backdate(&cache.join("sub"), 7200);
    // Written last, since adding entries updates a directory's mtime
    backdate(&cache, 7200);

    let job = run_older_than(base, 3600);

    // The directory's own mtime is old, but `sub/new.pyc` is not
    assert_eq!(job.counter, 0);
    assert!(cache.join("sub").join("new.pyc").exists());
}

#[test]
fn test_older_than_removes_directory_when_everything_is_old() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let cache = base.join("__pycache__");
    fs::create_dir_all(cache.join("sub")).unwrap();
    fs::write(cache.join("sub").join("a.pyc"), "old").unwrap();
    backdate(&cache.join("sub").join("a.pyc"), 7200);
    backdate(&cache.join("sub"), 7200);
    backdate(&cache, 7200);

    let job = run_older_than(base, 3600);

    assert_eq!(job.counter, 1);
    assert!(job.size > 0);
    assert!(!cache.exists());
}

#[test]
fn test_older_than_keeps_old_directory_with_a_new_subdirectory() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let cache = base.join("__pycache__");
    fs::create_dir_all(cache.join("empty")).unwrap();
    backdate(&cache, 7200);

    let job = run_older_than(base, 3600);

    assert_eq!(job.counter, 0);
    assert!(cache.join("empty").exists());
}

#[test]
fn test_older_than_keeps_future_timestamps() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let file = base.join("future.pyc");
    fs::write(&file, "x").unwrap();
    let later = SystemTime::now() + Duration::from_secs(86400);
    filetime::set_file_mtime(&file, filetime::FileTime::from_system_time(later)).unwrap();

    let job = run_older_than(base, 3600);

    assert_eq!(job.counter, 0);
    assert!(file.exists());
}

#[test]
fn test_older_than_beyond_the_clock_matches_nothing() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let file = base.join("old.pyc");
    fs::write(&file, "x").unwrap();
    backdate(&file, 7200);

    let job = run_older_than(base, u64::MAX);

    assert_eq!(job.counter, 0);
    assert!(file.exists());
}

/// Make `dir` unreadable, returning false when the permission has no effect
/// (running as root)
#[cfg(unix)]
fn lock(dir: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o000)).unwrap();
    fs::read_dir(dir).is_err()
}

#[cfg(unix)]
fn unlock(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
#[cfg(unix)]
fn test_unreadable_directory_is_a_warning() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let locked = base.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("hidden.pyc"), "x").unwrap();
    fs::write(base.join("seen.pyc"), "x").unwrap();
    if !lock(&locked) {
        unlock(&locked);
        return;
    }

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .json_mode(true)
        .build();
    let mut job = CleaningJob::new(config);
    let result = job.run();
    unlock(&locked);
    result.unwrap();

    assert_eq!(job.counter, 1);
    assert!(job.has_warnings());
    assert_eq!(job.warnings[0].0, locked);

    let json: serde_json::Value = serde_json::from_str(&job.to_json().unwrap()).unwrap();
    assert_eq!(json["warnings"].as_array().unwrap().len(), 1);
}

#[test]
#[cfg(unix)]
fn test_older_than_keeps_directory_with_unreadable_contents() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let cache = base.join("__pycache__");
    let locked = cache.join("locked");
    fs::create_dir_all(&locked).unwrap();
    backdate(&locked, 7200);
    backdate(&cache, 7200);
    if !lock(&locked) {
        unlock(&locked);
        return;
    }

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/__pycache__".to_string()])
        .skip_confirmation(true)
        .older_than_secs(Some(3600))
        .build();
    let mut job = CleaningJob::new(config);
    let result = job.run();
    unlock(&locked);
    result.unwrap();

    // The age of `locked`'s contents is unknown, so the target is kept
    assert_eq!(job.counter, 0);
    assert!(job.has_warnings());
    assert!(cache.exists());
}
