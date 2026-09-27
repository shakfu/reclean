use reclean::{parse_size, CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_size_units() {
    assert_eq!(parse_size("0").unwrap(), 0);
    assert_eq!(parse_size("4096").unwrap(), 4096);
    assert_eq!(parse_size("512B").unwrap(), 512);
    assert_eq!(parse_size("1K").unwrap(), 1024);
    assert_eq!(parse_size("1KiB").unwrap(), 1024);
    assert_eq!(parse_size("100M").unwrap(), 100 << 20);
    assert_eq!(parse_size("2G").unwrap(), 2 << 30);
    assert_eq!(parse_size("1T").unwrap(), 1 << 40);
}

#[test]
fn test_parse_size_fractions_are_exact_and_truncated() {
    assert_eq!(parse_size("1.5K").unwrap(), 1536);
    assert_eq!(parse_size(".5K").unwrap(), 512);
    assert_eq!(parse_size("1.K").unwrap(), 1024);
    // 0.001 KiB is 1.024 bytes
    assert_eq!(parse_size("0.001K").unwrap(), 1);
    assert_eq!(parse_size("1.9").unwrap(), 1);
}

#[test]
fn test_parse_size_rejects_malformed_input() {
    for bad in [
        "", ".", "K", "5X", "5k", "-5K", "+5K", "1e3", "1.2.3K", "5 K", "1.5KiBx",
    ] {
        assert!(parse_size(bad).is_err(), "{:?}", bad);
    }
    // Surrounding whitespace is trimmed, as `parse_duration` does
    assert_eq!(parse_size(" 5K ").unwrap(), 5120);
}

#[test]
fn test_parse_size_overflow_is_an_error() {
    assert_eq!(parse_size("18446744073709551615").unwrap(), u64::MAX);
    assert!(parse_size("18446744073709551616").is_err());
    assert!(parse_size("16777216T").is_err());
    assert!(parse_size("999999999999999999999999999999999999999T").is_err());
}

#[test]
fn test_larger_than_keeps_only_big_targets() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    fs::write(base.join("small.pyc"), vec![0u8; 100]).unwrap();
    fs::write(base.join("big.pyc"), vec![0u8; 5000]).unwrap();
    let cache = base.join("__pycache__");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("x.pyc"), vec![0u8; 3000]).unwrap();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string(), "**/__pycache__".to_string()])
        .larger_than_bytes(Some(parse_size("2K").unwrap()))
        .skip_confirmation(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    // The directory is judged by its total size, not its own entry
    assert_eq!(job.counter, 2);
    assert!(base.join("small.pyc").exists());
    assert!(!base.join("big.pyc").exists());
    assert!(!cache.exists());
}
