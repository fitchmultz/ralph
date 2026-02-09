//! Unit tests for timeutil.rs (timestamp parsing, formatting, edge cases).

use ralph::timeutil;
use std::thread;
use std::time::Duration;

#[test]
fn test_now_utc_rfc3339_success() {
    let result = timeutil::now_utc_rfc3339();
    assert!(result.is_ok());

    let timestamp = result.unwrap();
    // Should be a valid RFC3339 timestamp
    assert!(!timestamp.is_empty());
    assert!(timestamp.contains('T'));
    assert!(timestamp.contains('Z'));
}

#[test]
fn test_now_utc_rfc3339_format() {
    let result = timeutil::now_utc_rfc3339();
    assert!(result.is_ok());

    let timestamp = result.unwrap();
    // RFC3339 format with fixed fractional seconds: YYYY-MM-DDTHH:MM:SS.sssssssssZ
    // Example: 2025-01-19T12:34:56.123456789Z (length 30)
    assert_eq!(timestamp.len(), 30);
    assert!(timestamp.contains('T'));
    assert!(timestamp.ends_with('Z'));
    assert!(timestamp.contains('.'));

    // Date part: YYYY-MM-DD
    let date_part = &timestamp[0..10];
    assert_eq!(&date_part[4..5], "-");
    assert_eq!(&date_part[7..8], "-");

    // Time part before fractional seconds: HH:MM:SS
    let time_part = &timestamp[11..19];
    assert_eq!(&time_part[2..3], ":");
    assert_eq!(&time_part[5..6], ":");
}

#[test]
fn test_now_utc_rfc3339_monotonic() {
    let timestamp1 = timeutil::now_utc_rfc3339().unwrap();
    thread::sleep(Duration::from_millis(100));
    let timestamp2 = timeutil::now_utc_rfc3339().unwrap();

    // timestamp2 should be >= timestamp1
    assert!(timestamp2 >= timestamp1);
}

#[test]
fn test_now_utc_rfc3339_year_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let year_str = &timestamp[0..4];
    let year: i32 = year_str.parse().unwrap();

    // Year should be reasonable (2020-2030)
    assert!(year >= 2020);
    assert!(year <= 2030);
}

#[test]
fn test_now_utc_rfc3339_month_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let month_str = &timestamp[5..7];
    let month: u32 = month_str.parse().unwrap();

    // Month should be 01-12
    assert!(month >= 1);
    assert!(month <= 12);
}

#[test]
fn test_now_utc_rfc3339_day_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let day_str = &timestamp[8..10];
    let day: u32 = day_str.parse().unwrap();

    // Day should be 01-31
    assert!(day >= 1);
    assert!(day <= 31);
}

#[test]
fn test_now_utc_rfc3339_hour_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let hour_str = &timestamp[11..13];
    let hour: u32 = hour_str.parse().unwrap();

    // Hour should be 00-23
    assert!(hour <= 23);
}

#[test]
fn test_now_utc_rfc3339_minute_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let minute_str = &timestamp[14..16];
    let minute: u32 = minute_str.parse().unwrap();

    // Minute should be 00-59
    assert!(minute <= 59);
}

#[test]
fn test_now_utc_rfc3339_second_range() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();
    let second_str = &timestamp[17..19];
    let second: u32 = second_str.parse().unwrap();

    // Second should be 00-59
    assert!(second <= 59);
}

#[test]
fn test_now_utc_rfc3339_or_fallback_success() {
    let timestamp = timeutil::now_utc_rfc3339_or_fallback();
    assert!(!timestamp.is_empty());
    assert!(timestamp.contains('T'));
    assert!(timestamp.contains('Z'));
}

#[test]
fn test_fallback_constant() {
    // Fallback is the Unix epoch sentinel (obviously wrong for modern data)
    assert_eq!(timeutil::FALLBACK_RFC3339, "1970-01-01T00:00:00.000000000Z");
}

#[test]
fn test_fallback_constant_format() {
    let fallback = timeutil::FALLBACK_RFC3339;
    assert_eq!(fallback.len(), 30);
    assert_eq!(&fallback[10..11], "T");
    assert_eq!(&fallback[19..20], ".");
    assert_eq!(&fallback[29..30], "Z");
}

#[test]
fn test_now_utc_rfc3339_idempotent() {
    let timestamp1 = timeutil::now_utc_rfc3339().unwrap();
    thread::sleep(Duration::from_millis(50));
    let timestamp2 = timeutil::now_utc_rfc3339().unwrap();

    // Same format, different times
    assert_eq!(timestamp1.len(), timestamp2.len());
}

#[test]
fn test_now_utc_rfc3339_only_zulu() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();

    // Should end with Z (Zulu time/UTC)
    assert!(timestamp.ends_with('Z'));

    // Should not contain timezone offset
    assert!(!timestamp.contains('+'));
    assert!(!timestamp.contains("-00:00"));
    assert!(!timestamp.contains("+00:00"));
}

#[test]
fn test_now_utc_rfc3339_has_fractional_seconds() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();

    // RFC3339 format should include fixed fractional seconds
    // Format is: YYYY-MM-DDTHH:MM:SS.sssssssssZ
    assert!(timestamp.contains('.'));
}

#[test]
fn test_now_utc_rfc3339_ascii_only() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();

    // All characters should be ASCII
    assert!(timestamp.is_ascii());
}

#[test]
fn test_now_utc_rfc3339_parseable() {
    let timestamp = timeutil::now_utc_rfc3339().unwrap();

    // Should be parseable by timeutil helpers
    let parsed = timeutil::parse_rfc3339(&timestamp);
    assert!(parsed.is_ok());
}

#[test]
fn test_now_utc_rfc3339_or_fallback_never_empty() {
    // Even if system time fails, should return fallback
    let timestamp = timeutil::now_utc_rfc3339_or_fallback();
    assert!(!timestamp.is_empty());
    // Timestamp has fixed fractional seconds
    assert_eq!(timestamp.len(), 30);
}

#[test]
fn test_now_utc_rfc3339_consistent_length() {
    // Fixed fractional second precision should yield consistent lengths.
    let timestamps: Vec<_> = (0..5)
        .map(|_| timeutil::now_utc_rfc3339().unwrap())
        .collect();

    // All timestamps from this batch should have the same length
    let first_len = timestamps[0].len();
    for ts in &timestamps {
        assert_eq!(ts.len(), first_len);
    }
}

#[test]
fn test_fallback_timestamp_components() {
    let fallback = timeutil::FALLBACK_RFC3339;

    // Unix epoch: 1970-01-01T00:00:00.000000000Z
    // Year: 1970
    assert_eq!(&fallback[0..4], "1970");
    // Month: 01
    assert_eq!(&fallback[5..7], "01");
    // Day: 01
    assert_eq!(&fallback[8..10], "01");
    // Separator: T
    assert_eq!(&fallback[10..11], "T");
    // Hour: 00
    assert_eq!(&fallback[11..13], "00");
    // Minute: 00
    assert_eq!(&fallback[14..16], "00");
    // Second: 00
    assert_eq!(&fallback[17..19], "00");
    // Fractional seconds separator
    assert_eq!(&fallback[19..20], ".");
    // Zulu: Z
    assert_eq!(&fallback[29..30], "Z");
}

#[test]
fn test_parse_rfc3339_valid() {
    let ts = "2026-01-19T12:00:00Z";
    let result = timeutil::parse_rfc3339(ts);
    assert!(result.is_ok());
    let dt = result.unwrap();
    assert_eq!(dt.year(), 2026);
    assert_eq!(dt.month() as u8, 1);
    assert_eq!(dt.day(), 19);
}

#[test]
fn test_parse_rfc3339_invalid() {
    let ts = "invalid-timestamp";
    let result = timeutil::parse_rfc3339(ts);
    assert!(result.is_err());
}

#[test]
fn test_format_rfc3339_normalizes_to_utc() {
    let dt = timeutil::parse_rfc3339("2026-01-19T12:00:00-05:00").unwrap();
    let formatted = timeutil::format_rfc3339(dt).unwrap();
    assert_eq!(formatted, "2026-01-19T17:00:00.000000000Z");
}
