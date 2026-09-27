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

fn reclean(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reclean"))
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
    let out = reclean(
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
    let out = reclean(
        base,
        &["-c", config.to_str().unwrap(), "--glob", "**/*.log"],
    );

    assert!(out.status.success());
    assert!(!base.join("run.log").exists(), "--glob was ignored");
    assert!(base.join("mod.pyc").exists());
}

#[test]
fn test_configfile_mode_emits_json() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("mod.pyc"), "payload").unwrap();

    let config = write_config(base, base);
    let out = reclean(
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

    let out = reclean(base, &["-c", config.to_str().unwrap(), "-w"]);

    assert!(!out.status.success(), "conflicting flags were accepted");
    assert!(!base.join(".reclean.toml").exists());
}

#[test]
fn test_unknown_config_key_is_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("a.pyc"), "x").unwrap();
    let config = write_config(base, base);
    let mut text = fs::read_to_string(&config).unwrap();
    // A misspelt `dry_run` must not be ignored, or the run deletes
    text.push_str("dry_rn = true\n");
    fs::write(&config, text).unwrap();

    let out = reclean(base, &["-c", config.to_str().unwrap()]);

    assert_eq!(out.status.code(), Some(2));
    assert!(base.join("a.pyc").exists());
}

#[test]
fn test_bad_option_value_exits_2() {
    let temp_dir = TempDir::new().unwrap();
    let out = reclean(temp_dir.path(), &["-d", "--larger-than", "5X"]);
    assert_eq!(out.status.code(), Some(2));
    let out = reclean(temp_dir.path(), &["-d", "-o", "18446744073709551615w"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn test_missing_root_exits_1() {
    let temp_dir = TempDir::new().unwrap();
    let out = reclean(temp_dir.path(), &["-d", "-p", "does-not-exist"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
#[cfg(unix)]
fn test_unreadable_directory_exits_3() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let locked = base.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(base.join("a.pyc"), "x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        // Running as root: the permission has no effect
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let out = reclean(base, &["-y", "-g", "**/*.pyc"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(out.status.code(), Some(3));
    // The readable part of the tree was still cleaned
    assert!(!base.join("a.pyc").exists());
}

#[test]
fn test_clean_run_exits_0() {
    let temp_dir = TempDir::new().unwrap();
    let out = reclean(temp_dir.path(), &["-d"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn test_nested_repository_is_skipped_from_a_relative_root() {
    // The default root is `.`, so ancestors are walked as relative paths
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::create_dir(base.join(".git")).unwrap();
    let nested = base.join("sub");
    fs::create_dir_all(nested.join(".git")).unwrap();
    fs::write(nested.join("Cargo.toml"), "").unwrap();
    fs::create_dir(nested.join("target")).unwrap();

    let out = reclean(base, &["-y", "-B", "-g", "nothing"]);

    assert!(out.status.success());
    assert!(nested.join("target").exists());
}

/// Run reclean with `input` piped to stdin
fn reclean_with_input(cwd: &Path, args: &[&str], input: &[u8]) -> Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_reclean"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn test_piped_yes_confirms() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("a.pyc"), "x").unwrap();

    let out = reclean_with_input(base, &["-g", "**/*.pyc"], b"y\n");

    assert_eq!(out.status.code(), Some(0));
    assert!(!base.join("a.pyc").exists());
}

#[test]
fn test_piped_anything_else_cancels() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("a.pyc"), "x").unwrap();

    for input in [&b"n\n"[..], b"ny", b"\n", b""] {
        let out = reclean_with_input(base, &["-g", "**/*.pyc"], input);
        assert_eq!(out.status.code(), Some(0), "{:?}", input);
        assert!(base.join("a.pyc").exists(), "{:?}", input);
    }
}

#[test]
fn test_matched_list_shows_sizes_total_and_restore() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("a.pyc"), vec![0u8; 2048]).unwrap();
    fs::create_dir_all(base.join("node_modules").join("pkg")).unwrap();
    fs::write(base.join("node_modules").join("pkg").join("i.js"), "x").unwrap();
    fs::write(base.join("package-lock.json"), "").unwrap();

    let out = reclean(base, &["-d", "-D", "-g", "**/*.pyc"]);
    let err = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success());
    assert!(err.contains("2.00 KiB"), "{}", err);
    assert!(err.contains("(restore: npm ci)"), "{}", err);
    assert!(err.contains("2 target(s)"), "{}", err);
    // Two reasons, so a breakdown follows the total
    assert!(err.contains("dependency"), "{}", err);
}

#[test]
fn test_many_targets_are_all_removed() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    for i in 0..200 {
        let dir = base.join(format!("p{}", i)).join("__pycache__");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("m.pyc"), "x").unwrap();
    }

    let out = reclean(base, &["-y"]);

    assert_eq!(out.status.code(), Some(0));
    for i in 0..200 {
        assert!(!base.join(format!("p{}", i)).join("__pycache__").exists());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Deleted 200 item(s)"), "{}", err);
}

#[test]
fn test_configfile_path_is_reported_in_json() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let config = write_config(base, base);

    let out = reclean(
        base,
        &["-c", config.to_str().unwrap(), "-d", "--format", "json"],
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(json["config"], config.to_str().unwrap());
}
