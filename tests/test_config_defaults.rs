use reclean::constants::{get_default_excludes, get_default_patterns, get_protected_dirs};
use reclean::CleanConfig;

fn parse(text: &str) -> CleanConfig {
    toml::from_str(text).unwrap()
}

#[test]
fn test_empty_file_takes_every_default() {
    let config = parse("");
    assert_eq!(config.path, ".");
    assert_eq!(config.patterns, get_default_patterns());
    assert_eq!(config.exclude_patterns, get_default_excludes());
    assert_eq!(config.protected_dirs, get_protected_dirs());
    assert!(!config.dry_run);
    assert!(!config.skip_confirmation);
    assert!(!config.include_symlinks);
    assert!(!config.remove_broken_symlinks);
    assert!(!config.stats_mode);
    assert!(!config.build_artifacts);
    assert!(!config.dependencies);
    assert_eq!(config.older_than_secs, None);
    assert_eq!(config.larger_than_bytes, None);
}

#[test]
fn test_partial_file_keeps_given_keys() {
    let config = parse("patterns = [\"**/*.log\"]\ndry_run = true\n");
    assert_eq!(config.patterns, vec!["**/*.log".to_string()]);
    assert!(config.dry_run);
    assert!(!config.skip_confirmation);
    assert_eq!(config.path, ".");
}

#[test]
fn test_explicit_empty_patterns_match_nothing() {
    assert!(parse("patterns = []\n").patterns.is_empty());
}

#[test]
fn test_unknown_keys_are_still_rejected() {
    assert!(toml::from_str::<CleanConfig>("dry_runn = true\n").is_err());
}
