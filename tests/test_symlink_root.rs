#![cfg(unix)]

use reclean::{CleanConfig, CleaningJob};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A tree `real/{a.pyc, sub/b.pyc}` with `link -> real` beside it.
fn setup() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::create_dir_all(base.join("real").join("sub")).unwrap();
    fs::write(base.join("real").join("a.pyc"), "x").unwrap();
    fs::write(base.join("real").join("sub").join("b.pyc"), "x").unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    temp_dir
}

fn job(root: &Path, pattern: &str, include_symlinks: bool, dry_run: bool) -> CleaningJob {
    let config = CleanConfig::builder()
        .path(root.to_str().unwrap())
        .patterns(vec![pattern.to_string()])
        .include_symlinks(include_symlinks)
        .dry_run(dry_run)
        .skip_confirmation(true)
        .build();
    CleaningJob::new(config)
}

#[test]
fn test_symlinked_root_is_walked() {
    let temp_dir = setup();
    let link = temp_dir.path().join("link");

    for include_symlinks in [false, true] {
        let mut job = job(&link, "**/*.pyc", include_symlinks, true);
        job.run().unwrap();
        assert_eq!(job.counter, 2, "include_symlinks={}", include_symlinks);
        assert!(job.warnings.is_empty(), "{:?}", job.warnings);
    }
}

#[test]
fn test_symlinked_root_removes_contents_and_keeps_link() {
    let temp_dir = setup();
    let base = temp_dir.path();

    job(&base.join("link"), "**/*.pyc", true, false)
        .run()
        .unwrap();

    assert!(!base.join("real").join("a.pyc").exists());
    assert!(!base.join("real").join("sub").join("b.pyc").exists());
    assert!(fs::symlink_metadata(base.join("link"))
        .unwrap()
        .is_symlink());
}

#[test]
fn test_symlinked_root_matching_a_pattern_is_not_collected() {
    let temp_dir = setup();
    let base = temp_dir.path();

    // `**/link` names the root itself; only its contents may be targets
    let mut job = job(&base.join("link"), "**/link", true, false);
    job.run().unwrap();

    assert_eq!(job.counter, 0);
    assert!(fs::symlink_metadata(base.join("link"))
        .unwrap()
        .is_symlink());
    assert!(base.join("real").join("a.pyc").exists());
}
