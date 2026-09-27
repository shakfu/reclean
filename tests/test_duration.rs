use reclean::parse_duration;

#[test]
fn test_parse_duration_seconds() {
    assert_eq!(parse_duration("30s").unwrap(), 30);
    assert_eq!(parse_duration("1s").unwrap(), 1);
    assert_eq!(parse_duration("0s").unwrap(), 0);
}

#[test]
fn test_parse_duration_minutes() {
    assert_eq!(parse_duration("5m").unwrap(), 300);
    assert_eq!(parse_duration("1m").unwrap(), 60);
}

#[test]
fn test_parse_duration_hours() {
    assert_eq!(parse_duration("24h").unwrap(), 86400);
    assert_eq!(parse_duration("1h").unwrap(), 3600);
}

#[test]
fn test_parse_duration_days() {
    assert_eq!(parse_duration("30d").unwrap(), 2592000);
    assert_eq!(parse_duration("7d").unwrap(), 604800);
    assert_eq!(parse_duration("1d").unwrap(), 86400);
}

#[test]
fn test_parse_duration_weeks() {
    assert_eq!(parse_duration("2w").unwrap(), 1209600);
    assert_eq!(parse_duration("1w").unwrap(), 604800);
}

#[test]
fn test_parse_duration_empty_string() {
    assert!(parse_duration("").is_err());
    assert!(parse_duration("  ").is_err());
}

#[test]
fn test_parse_duration_no_unit() {
    // Single character like "5" with no unit
    assert!(parse_duration("5").is_err());
}

#[test]
fn test_parse_duration_invalid_unit() {
    assert!(parse_duration("30x").is_err());
    assert!(parse_duration("10y").is_err());
}

#[test]
fn test_parse_duration_invalid_number() {
    assert!(parse_duration("abcd").is_err());
    assert!(parse_duration("12.5d").is_err());
}

#[test]
fn test_parse_duration_whitespace_trimmed() {
    assert_eq!(parse_duration("  30d  ").unwrap(), 2592000);
}

#[test]
fn test_parse_duration_overflow_is_an_error() {
    // Wrapping here would turn a huge age into a small one
    assert!(parse_duration("18446744073709551615w").is_err());
    assert!(parse_duration("30500568904944s").is_ok());
    assert!(parse_duration("30500568904944w").is_err());
    assert_eq!(parse_duration("18446744073709551615s").unwrap(), u64::MAX);
}

#[test]
fn test_parse_duration_rejects_signs() {
    assert!(parse_duration("+5d").is_err());
    assert!(parse_duration("-5d").is_err());
}

#[test]
fn test_parse_duration_multibyte_unit_is_an_error() {
    // Used to panic slicing inside a multibyte char
    assert!(parse_duration("5\u{e9}").is_err());
    assert!(parse_duration("\u{e9}").is_err());
}
