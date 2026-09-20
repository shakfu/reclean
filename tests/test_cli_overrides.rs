//! CLI-level checks that config-file mode obeys the documented flag precedence.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Write a config naming `path` and matching `*.pyc`, and return its path
fn write_config(dir: &Path, target: &Path) -> std::path::PathBuf {
    let config = dir.join("config.toml");
    fs::write(
        &config,
        format!(
            r#"
path = "{}"
patterns = ["**/*.pyc"]
dry_run = false
skip_confirmation = true
include_symlinks = false
remove_broken_symlinks = false
"#,
            target.display()
        ),
    )
    .unwrap();
    config
}

fn rclean(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rclean"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn test_configfile_mode_honours_path_override() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let (from_config, from_cli) = (base.join("a"), base.join("b"));
    for dir in [&from_config, &from_cli] {
        fs::create_dir(dir).unwrap();
        fs::write(dir.join("mod.pyc"), "payload").unwrap();
    }

    let config = write_config(base, &from_config);
    let out = rclean(
        base,
        &[
            "-c",
            config.to_str().unwrap(),
            "--path",
            from_cli.to_str().unwrap(),
        ],
    );

    assert!(out.status.success());
    assert!(!from_cli.join("mod.pyc").exists(), "--path was ignored");
    assert!(from_config.join("mod.pyc").exists());
}

#[test]
fn test_configfile_mode_honours_glob_override() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("mod.pyc"), "payload").unwrap();
    fs::write(base.join("run.log"), "payload").unwrap();

    let config = write_config(base, base);
    let out = rclean(
        base,
        &["-c", config.to_str().unwrap(), "--glob", "**/*.log"],
    );

    assert!(out.status.success());
    assert!(!base.join("run.log").exists(), "--glob was ignored");
    assert!(base.join("mod.pyc").exists());
}

#[test]
fn test_configfile_mode_honours_preset_override() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("mod.pyc"), "payload").unwrap();
    fs::write(base.join(".DS_Store"), "payload").unwrap();

    let config = write_config(base, base);
    let out = rclean(
        base,
        &["-c", config.to_str().unwrap(), "--preset", "common"],
    );

    assert!(out.status.success());
    assert!(!base.join(".DS_Store").exists(), "--preset was ignored");
    assert!(base.join("mod.pyc").exists());
}

#[test]
fn test_configfile_mode_emits_json() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("mod.pyc"), "payload").unwrap();

    let config = write_config(base, base);
    let out = rclean(
        base,
        &[
            "-c",
            config.to_str().unwrap(),
            "--dry-run",
            "--format",
            "json",
        ],
    );

    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is not JSON");
    assert_eq!(json["summary"]["total_count"], 1);
    assert_eq!(json["summary"]["dry_run"], true);
    assert!(base.join("mod.pyc").exists());
}

#[test]
fn test_write_configfile_conflicts_with_configfile() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let config = write_config(base, base);

    let out = rclean(base, &["-c", config.to_str().unwrap(), "-w"]);

    assert!(!out.status.success(), "conflicting flags were accepted");
    assert!(!base.join(".rclean.toml").exists());
}
