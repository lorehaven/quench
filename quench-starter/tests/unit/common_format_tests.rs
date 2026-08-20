//! Unit tests for `common/format.rs`.

use quench_starter::common::format::human_bytes;

#[test]
fn bytes_under_a_kilobyte_render_as_a_bare_byte_count() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(512), "512 B");
    assert_eq!(human_bytes(1023), "1023 B");
}

#[test]
fn kilobyte_range_rounds_to_a_whole_number() {
    assert_eq!(human_bytes(1024), "1 KB");
    assert_eq!(human_bytes(1024 * 7), "7 KB");
    assert_eq!(human_bytes(1024 * 1024 - 1), "1024 KB");
}

#[test]
fn megabyte_and_above_keeps_one_decimal_place() {
    assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(human_bytes((1024.0 * 1024.0 * 3.4) as i64), "3.4 MB");
}
