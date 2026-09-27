use reclean::constants::{get_default_patterns, DEPENDENCIES};

#[test]
fn test_default_patterns_cover_python_caches() {
    let defaults = get_default_patterns();
    assert!(defaults.contains(&"**/__pycache__".to_string()));
    assert!(defaults.contains(&"**/*.pyc".to_string()));
}

#[test]
fn test_default_patterns_no_duplicates() {
    let defaults = get_default_patterns();
    let mut seen = std::collections::HashSet::new();
    for pattern in &defaults {
        assert!(
            seen.insert(pattern),
            "Duplicate pattern in defaults: {}",
            pattern
        );
    }
}

#[test]
fn test_default_patterns_never_name_a_dependency_tree() {
    // These are matched only beside their lock file, under --dependencies
    let defaults = get_default_patterns();
    for (dir, _, _) in DEPENDENCIES {
        assert!(
            !defaults.iter().any(|p| p.ends_with(dir)),
            "default pattern matches {} by name",
            dir
        );
    }
}

#[test]
fn test_every_dependency_has_a_restore_command() {
    for (dir, marker, restore) in DEPENDENCIES {
        assert!(!dir.is_empty() && !marker.is_empty() && !restore.is_empty());
    }
}

#[test]
fn test_default_patterns_spare_vim_swap_files() {
    // A swap file is the recovery copy of unsaved edits
    let defaults = get_default_patterns();
    assert!(!defaults.contains(&"**/*.swp".to_string()));
    assert!(!defaults.contains(&"**/*.swo".to_string()));
}
