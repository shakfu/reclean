use reclean::{CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

/// The only test in this file: it changes the process working directory, which
/// would race against tests running beside it in the same binary.
#[test]
fn test_relative_paths_are_walked() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    fs::create_dir_all(base.join("pkg").join("__pycache__")).unwrap();
    fs::write(base.join("pkg").join("__pycache__").join("m.pyc"), "x").unwrap();
    fs::write(base.join("root.pyc"), "x").unwrap();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(base).unwrap();

    // "." is the default path, and `should_process` rejects it by name; the walk
    // must still descend into it
    for path in [".", "./pkg", "pkg"] {
        let config = CleanConfig::builder()
            .path(path)
            .patterns(vec!["**/*.pyc".to_string()])
            .dry_run(true)
            .skip_confirmation(true)
            .build();
        let mut job = CleaningJob::new(config);
        job.run().unwrap();
        let expected = if path == "." { 2 } else { 1 };
        assert_eq!(job.counter, expected, "path {:?} matched nothing", path);
    }

    std::env::set_current_dir(original).unwrap();
}
