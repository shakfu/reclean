use reclean::format_size;

#[test]
fn test_format_size_bytes() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1), "1 B");
    assert_eq!(format_size(512), "512 B");
    assert_eq!(format_size(1023), "1023 B");
}

#[test]
fn test_format_size_kib() {
    assert_eq!(format_size(1024), "1.00 KiB");
    assert_eq!(format_size(1536), "1.50 KiB");
    assert_eq!(format_size(10240), "10.00 KiB");
}

#[test]
fn test_format_size_mib() {
    assert_eq!(format_size(1048576), "1.00 MiB");
    assert_eq!(format_size(1572864), "1.50 MiB");
    assert_eq!(format_size(10485760), "10.00 MiB");
}

#[test]
fn test_format_size_gib() {
    assert_eq!(format_size(1073741824), "1.00 GiB");
    assert_eq!(format_size(2147483648), "2.00 GiB");
}

#[test]
fn test_format_size_tib() {
    assert_eq!(format_size(1099511627776), "1.00 TiB");
}
