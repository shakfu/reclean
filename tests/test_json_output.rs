use reclean::{CleanConfig, CleaningJob};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_json_output_structure() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    fs::write(base.join("test.pyc"), "compiled python").unwrap();
    fs::write(base.join("keep.txt"), "keep this").unwrap();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .stats_mode(true)
        .json_mode(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    let json_str = job.to_json().unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    // Check top-level structure
    assert!(json["matches"].is_array());
    assert!(json["summary"].is_object());
    assert!(json["stats"].is_array());
    assert!(json["failures"].is_array());

    // Check summary
    assert_eq!(json["summary"]["total_count"], 1);
    assert!(json["summary"]["total_size"].as_u64().unwrap() > 0);
    assert_eq!(json["summary"]["dry_run"], true);

    // Check matches
    let matches = json["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["path"].as_str().unwrap().contains("test.pyc"));
    assert_eq!(matches[0]["pattern"], "**/*.pyc");

    // Check stats
    let stats = json["stats"].as_array().unwrap();
    assert!(!stats.is_empty());
}

#[test]
fn test_json_output_no_matches() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    fs::write(base.join("keep.txt"), "keep this").unwrap();

    let config = CleanConfig::builder()
        .path(base.to_str().unwrap())
        .patterns(vec!["**/*.pyc".to_string()])
        .dry_run(true)
        .skip_confirmation(true)
        .json_mode(true)
        .build();
    let mut job = CleaningJob::new(config);
    job.run().unwrap();

    let json_str = job.to_json().unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(json["summary"]["total_count"], 0);
    assert_eq!(json["matches"].as_array().unwrap().len(), 0);
}
