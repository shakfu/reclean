use reclean::{discover_config, find_config_upward};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_find_config_upward_in_current_dir() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".reclean.toml");
    fs::write(&config_path, "path = \".\"").unwrap();

    let result = find_config_upward(temp_dir.path(), ".reclean.toml");
    assert_eq!(result, Some(config_path));
}

#[test]
fn test_find_config_upward_in_parent_dir() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".reclean.toml");
    fs::write(&config_path, "path = \".\"").unwrap();

    let child = temp_dir.path().join("subdir");
    fs::create_dir(&child).unwrap();

    let result = find_config_upward(&child, ".reclean.toml");
    assert_eq!(result, Some(config_path));
}

#[test]
fn test_find_config_upward_in_grandparent_dir() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".reclean.toml");
    fs::write(&config_path, "path = \".\"").unwrap();

    let child = temp_dir.path().join("a").join("b");
    fs::create_dir_all(&child).unwrap();

    let result = find_config_upward(&child, ".reclean.toml");
    assert_eq!(result, Some(config_path));
}

#[test]
fn test_find_config_upward_not_found() {
    let temp_dir = TempDir::new().unwrap();
    // No config file created
    let result = find_config_upward(temp_dir.path(), ".reclean.toml");
    assert!(result.is_none());
}

#[test]
fn test_find_config_upward_ignores_directories() {
    let temp_dir = TempDir::new().unwrap();
    // Create a directory with the config filename (should be ignored)
    let config_dir = temp_dir.path().join(".reclean.toml");
    fs::create_dir(&config_dir).unwrap();

    let result = find_config_upward(temp_dir.path(), ".reclean.toml");
    assert!(result.is_none());
}

#[test]
fn test_discover_config_finds_local_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".reclean.toml");
    fs::write(&config_path, "path = \".\"").unwrap();

    let result = discover_config(temp_dir.path());
    assert_eq!(result, Some(config_path));
}

#[test]
fn test_discover_config_prefers_local_over_global() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".reclean.toml");
    fs::write(&config_path, "path = \".\"").unwrap();

    // discover_config should find the local file first, before checking global
    let result = discover_config(temp_dir.path());
    assert_eq!(result, Some(config_path));
}
