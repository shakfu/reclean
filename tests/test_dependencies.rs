use reclean::{CleanConfig, CleaningJob, Reason};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// A dependency tree `dir` holding one file, with `lock` beside it when given
fn create_tree(base: &Path, dir: &str, lock: Option<&str>) {
    let tree = base.join(dir).join("pkg");
    fs::create_dir_all(&tree).unwrap();
    fs::write(tree.join("index.js"), "payload").unwrap();
    if let Some(lock) = lock {
        fs::write(base.join(lock), "").unwrap();
    }
}

fn run(path: &Path, dependencies: bool, excludes: Option<Vec<String>>) -> CleaningJob {
    let mut builder = CleanConfig::builder()
        .path(path.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dependencies(dependencies)
        .skip_confirmation(true)
        .json_mode(true);
    if let Some(excludes) = excludes {
        builder = builder.exclude_patterns(excludes);
    }
    let mut job = CleaningJob::new(builder.build());
    job.run().unwrap();
    job
}

#[test]
fn test_dependency_beside_its_lock_is_matched() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("package-lock.json"));

    let job = run(base, true, None);

    assert_eq!(job.counter, 1);
    assert_eq!(job.matched_items[0].reason, Reason::Dependency);
    assert_eq!(job.matched_items[0].restore, Some("npm ci"));
    assert!(!base.join("node_modules").exists());
}

#[test]
fn test_dependencies_need_the_flag() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("package-lock.json"));

    let job = run(base, false, None);

    assert_eq!(job.counter, 0);
    assert!(base.join("node_modules").exists());
}

#[test]
fn test_dependency_without_a_lock_is_left_alone() {
    // package.json carries ranges; nothing on disk says what tree to restore
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("package.json"));

    let job = run(base, true, None);

    assert_eq!(job.counter, 0);
    assert!(base.join("node_modules").exists());
}

#[test]
fn test_lock_must_pair_with_its_directory() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "vendor", Some("package-lock.json"));

    let job = run(base, true, None);

    assert_eq!(job.counter, 0);
    assert!(base.join("vendor").exists());
}

#[test]
#[cfg(unix)]
fn test_symlinked_lock_does_not_count() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", None);
    let elsewhere = TempDir::new().unwrap();
    fs::write(elsewhere.path().join("package-lock.json"), "").unwrap();
    std::os::unix::fs::symlink(
        elsewhere.path().join("package-lock.json"),
        base.join("package-lock.json"),
    )
    .unwrap();

    let job = run(base, true, None);

    assert_eq!(job.counter, 0);
}

#[test]
fn test_venv_beside_uv_lock_overrides_the_default_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, ".venv", Some("uv.lock"));

    let job = run(base, true, None);

    assert_eq!(job.counter, 1);
    assert_eq!(job.matched_items[0].restore, Some("uv sync"));
    assert!(!base.join(".venv").exists());
}

#[test]
fn test_venv_without_uv_lock_stays_excluded() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, ".venv", None);
    fs::write(base.join(".venv").join("mod.pyc"), "x").unwrap();

    let job = run(base, true, None);

    // Neither removed whole nor entered
    assert_eq!(job.counter, 0);
    assert!(base.join(".venv").join("mod.pyc").exists());
}

#[test]
fn test_configured_exclude_applies_to_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("yarn.lock"));

    let job = run(base, true, Some(vec!["**/node_modules".to_string()]));

    assert_eq!(job.counter, 0);
    assert!(base.join("node_modules").exists());
}

#[test]
fn test_nested_dependency_trees_are_one_target() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("pnpm-lock.yaml"));
    let inner = base.join("node_modules").join("pkg");
    create_tree(&inner, "node_modules", Some("pnpm-lock.yaml"));

    let job = run(base, true, None);

    assert_eq!(job.counter, 1);
}

#[test]
fn test_json_reports_reason_type_and_restore() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    create_tree(base, "node_modules", Some("bun.lock"));
    fs::write(base.join("a.pyc"), "x").unwrap();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dependencies(true)
        .dry_run(true)
        .json_mode(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();
    let json: serde_json::Value = serde_json::from_str(&job.to_json().unwrap()).unwrap();

    assert_eq!(json["schema"], 1);
    assert!(json["config"].is_null());
    let matches = json["matches"].as_array().unwrap();
    let dep = matches
        .iter()
        .find(|m| m["reason"] == "dependency")
        .unwrap();
    assert_eq!(dep["type"], "directory");
    assert_eq!(dep["restore"], "bun install --frozen-lockfile");
    let pyc = matches.iter().find(|m| m["reason"] == "pattern").unwrap();
    assert_eq!(pyc["type"], "file");
    assert!(pyc.get("restore").is_none());

    // Cheapest to restore first
    let reasons: Vec<_> = json["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["reason"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(reasons, ["pattern", "dependency"]);
}
