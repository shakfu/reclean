use reclean::{CleanConfig, CleaningJob};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A project root: `.git`, a marker file, and the build directory it explains
fn create_project(base: &Path, marker: &str, artifact: &str) {
    fs::create_dir_all(base).unwrap();
    fs::create_dir(base.join(".git")).unwrap();
    fs::write(base.join(marker), "").unwrap();

    let out = base.join(artifact).join("obj");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("a.o"), "payload").unwrap();
}

fn run(path: &Path, build_artifacts: bool) -> CleaningJob {
    let config = CleanConfig::builder()
        .path(path.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .build_artifacts(build_artifacts)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();
    job
}

#[test]
fn test_build_artifacts_are_matched_at_the_project_root() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "CMakeLists.txt", "build");

    let job = run(base, true);

    // The directory is one target, and the walk does not descend into it
    assert_eq!(job.counter, 1);
    assert!(!base.join("build").exists());
}

#[test]
fn test_nested_build_dir_is_not_matched() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "CMakeLists.txt", "build");

    // A CMake subdirectory carries its own CMakeLists.txt, but no .git
    let nested = base.join("src").join("program");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("CMakeLists.txt"), "").unwrap();
    fs::create_dir(nested.join("build")).unwrap();
    fs::write(nested.join("build").join("a.o"), "payload").unwrap();

    let job = run(base, true);

    assert_eq!(job.counter, 1);
    assert!(!base.join("build").exists());
    assert!(nested.join("build/a.o").exists());
}

#[test]
fn test_build_artifacts_need_the_flag() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "CMakeLists.txt", "build");

    let job = run(base, false);

    assert_eq!(job.counter, 0);
    assert!(base.join("build/obj/a.o").exists());
}

#[test]
fn test_marker_without_git_is_not_a_project_root() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "CMakeLists.txt", "build");
    fs::remove_dir(base.join(".git")).unwrap();

    let job = run(base, true);

    assert_eq!(job.counter, 0);
    assert!(base.join("build/obj/a.o").exists());
}

#[test]
fn test_git_without_a_marker_is_not_enough() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "CMakeLists.txt", "build");
    fs::remove_file(base.join("CMakeLists.txt")).unwrap();

    let job = run(base, true);

    assert_eq!(job.counter, 0);
    assert!(base.join("build/obj/a.o").exists());
}

#[test]
fn test_directory_and_marker_must_be_a_pair() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    // `target` is Rust and Maven output; a Node project does not produce one
    create_project(base, "package.json", "target");

    let job = run(base, true);

    assert_eq!(job.counter, 0);
    assert!(base.join("target/obj/a.o").exists());

    fs::write(base.join("Cargo.toml"), "").unwrap();
    let job = run(base, true);

    assert_eq!(job.counter, 1);
    assert!(!base.join("target").exists());
}

#[test]
fn test_git_file_marks_a_submodule_root() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "Cargo.toml", "target");

    // A submodule marks its git directory with a file, not a directory
    fs::remove_dir(base.join(".git")).unwrap();
    fs::write(base.join(".git"), "gitdir: ../.git/modules/sub").unwrap();

    let job = run(base, true);

    assert_eq!(job.counter, 1);
    assert!(!base.join("target").exists());
}

#[test]
fn test_excluded_projects_keep_their_build_output() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(
        &base.join(".venv").join("src").join("pkg"),
        "setup.py",
        "build",
    );

    let job = run(base, true);

    assert_eq!(job.counter, 0);
    assert!(base.join(".venv/src/pkg/build/obj/a.o").exists());
}

#[test]
fn test_artifact_matches_are_named_in_stats() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(base, "pyproject.toml", "dist");

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .build_artifacts(true)
        .stats_mode(true)
        .dry_run(true)
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    assert_eq!(job.stats.get("build-artifact").map(|(n, _)| *n), Some(1));
}

#[test]
fn test_nested_repository_build_output_is_not_matched() {
    // A submodule or vendored checkout inside an outer repository
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::create_dir(base.join(".git")).unwrap();
    let nested = base.join("vendor").join("lib");
    create_project(&nested, "Cargo.toml", "target");

    let job = run(base, true);

    assert_eq!(job.counter, 0);
    assert!(nested.join("target").exists());
}

#[test]
fn test_sibling_projects_under_a_plain_directory_are_matched() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_project(&base.join("a"), "Cargo.toml", "target");
    create_project(&base.join("b"), "CMakeLists.txt", "build");

    let job = run(base, true);

    assert_eq!(job.counter, 2);
    assert!(!base.join("a").join("target").exists());
    assert!(!base.join("b").join("build").exists());
}

#[test]
fn test_nested_repository_is_matched_when_named_as_root() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::create_dir(base.join(".git")).unwrap();
    let nested = base.join("vendor").join("lib");
    create_project(&nested, "Cargo.toml", "target");

    let job = run(&nested, true);

    assert_eq!(job.counter, 1);
    assert!(!nested.join("target").exists());
}
