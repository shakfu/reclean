use reclean::{CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

/// The only test in this file: it changes the process working directory, which
/// would race against tests running beside it in the same binary.
#[test]
fn test_parent_relative_roots_are_walked() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    fs::create_dir_all(base.join("proj").join("sub")).unwrap();
    fs::create_dir_all(base.join("sib")).unwrap();
    fs::write(base.join("proj").join("a.pyc"), "x").unwrap();
    fs::write(base.join("proj").join("sub").join("b.pyc"), "x").unwrap();
    fs::write(base.join("sib").join("c.pyc"), "x").unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(base.join("proj").join("sub")).unwrap();

    // Every spelling of a root reached through `..` must be walked, not skipped
    for (path, expected) in [("..", 2), ("../../sib", 1), ("./../../sib", 1)] {
        let config = CleanConfig::builder()
            .path(path)
            .patterns(vec!["**/*.pyc".to_string()])
            .dry_run(true)
            .skip_confirmation(true)
            .build();
        let mut job = CleaningJob::new(config);
        job.run().unwrap();
        assert_eq!(job.counter, expected, "path {:?}", path);
        assert!(
            job.warnings.is_empty(),
            "path {:?}: {:?}",
            path,
            job.warnings
        );
    }

    // A real run removes what the dry run reported
    let config = CleanConfig::builder()
        .path("..")
        .patterns(vec!["**/*.pyc".to_string()])
        .skip_confirmation(true)
        .build();
    CleaningJob::new(config).run().unwrap();

    std::env::set_current_dir(original).unwrap();

    assert!(!base.join("proj").join("a.pyc").exists());
    assert!(!base.join("proj").join("sub").join("b.pyc").exists());
    assert!(base.join("sib").join("c.pyc").exists());
}
