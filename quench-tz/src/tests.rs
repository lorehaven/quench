use super::*;
use quench_core::date::Date;

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> Time {
    Time::from_ymd_hms(y, mo, d, h, mi, s).expect("a real time")
}

fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> Civil {
    Date::from_ymd(y, mo, d).expect("a real date").at(h, mi, 0)
}

fn warsaw() -> Zone {
    Zone::named("Europe/Warsaw").expect("in the database")
}

#[quench_test::test]
fn offsets_follow_daylight_saving() {
    let w = warsaw();
    let winter = w.offset_at(utc(2026, 1, 15, 12, 0, 0));
    assert_eq!(
        (
            winter.utc_offset,
            winter.is_dst,
            winter.abbreviation.as_str()
        ),
        (3600, false, "CET")
    );
    let summer = w.offset_at(utc(2026, 7, 15, 12, 0, 0));
    assert_eq!(
        (
            summer.utc_offset,
            summer.is_dst,
            summer.abbreviation.as_str()
        ),
        (7200, true, "CEST")
    );
    // The switch is at 01:00 UTC on the last Sunday of March.
    assert!(!w.offset_at(utc(2026, 3, 29, 0, 59, 59)).is_dst);
    assert!(w.offset_at(utc(2026, 3, 29, 1, 0, 0)).is_dst);
    // Far past the listed transitions the footer rule keeps going.
    assert!(w.offset_at(utc(2090, 7, 1, 0, 0, 0)).is_dst);
    assert!(!w.offset_at(utc(2090, 1, 1, 0, 0, 0)).is_dst);
    // Before standard time existed: local mean time.
    assert_eq!(w.offset_at(utc(1850, 1, 1, 0, 0, 0)).abbreviation, "LMT");
}

#[quench_test::test]
fn local_fields_and_the_other_direction() {
    let w = warsaw();
    let at = utc(2026, 7, 1, 12, 0, 0);
    let c = w.civil_at(at);
    assert_eq!(
        (c.year, c.month, c.day, c.hour, c.minute),
        (2026, 7, 1, 14, 0)
    );
    assert_eq!(w.resolve(&c), Resolved::Single(at));

    let across_midnight = w.civil_at(utc(2026, 12, 31, 23, 30, 0));
    assert_eq!(
        (
            across_midnight.year,
            across_midnight.month,
            across_midnight.day,
            across_midnight.hour
        ),
        (2027, 1, 1, 0)
    );

    let with_nanos = Civil {
        nanosecond: 500,
        ..c
    };
    assert_eq!(
        w.resolve(&with_nanos).single().map(|t| t.subsec_nanos()),
        Some(500)
    );
}

#[quench_test::test]
fn clocks_forward_leave_a_gap_and_clocks_back_repeat_an_hour() {
    let w = warsaw();
    // 2026-03-29: 02:00 CET jumps to 03:00 CEST.
    assert_eq!(w.resolve(&local(2026, 3, 29, 2, 30)), Resolved::Gap);
    assert!(w.resolve(&local(2026, 3, 29, 1, 59)).single().is_some());
    assert!(w.resolve(&local(2026, 3, 29, 3, 0)).single().is_some());
    // 2026-10-25: 03:00 CEST falls back to 02:00 CET, so 02:30 happens twice.
    let Resolved::Ambiguous { earlier, later } = w.resolve(&local(2026, 10, 25, 2, 30)) else {
        panic!("2:30 repeats");
    };
    assert_eq!(later.timestamp() - earlier.timestamp(), 3600);
    assert_eq!(w.offset_at(earlier).abbreviation, "CEST");
    assert_eq!(w.offset_at(later).abbreviation, "CET");

    let repeated = w.resolve(&local(2026, 10, 25, 2, 30));
    assert_eq!(
        (repeated.earliest(), repeated.latest()),
        (Some(earlier), Some(later))
    );
    assert_eq!(repeated.single(), None);
    let gap = w.resolve(&local(2026, 3, 29, 2, 30));
    assert_eq!(
        (gap.earliest(), gap.latest(), gap.single()),
        (None, None, None)
    );
}

#[quench_test::test]
fn unusual_zones() {
    // Half-hour and 45-minute offsets, southern-hemisphere daylight saving, no daylight saving.
    let kolkata = Zone::named("Asia/Kolkata").unwrap();
    assert_eq!(
        kolkata.offset_at(utc(2026, 1, 1, 0, 0, 0)).utc_offset,
        19_800
    );
    let kathmandu = Zone::named("Asia/Kathmandu").unwrap();
    assert_eq!(
        kathmandu.offset_at(utc(2026, 1, 1, 0, 0, 0)).utc_offset,
        20_700
    );
    let sydney = Zone::named("Australia/Sydney").unwrap();
    assert!(sydney.offset_at(utc(2026, 1, 15, 0, 0, 0)).is_dst);
    assert!(!sydney.offset_at(utc(2026, 7, 15, 0, 0, 0)).is_dst);
    let tokyo = Zone::named("Asia/Tokyo").unwrap();
    assert_eq!(
        tokyo.offset_at(utc(2026, 7, 1, 0, 0, 0)).abbreviation,
        "JST"
    );
    let new_york = Zone::named("America/New_York").unwrap();
    assert_eq!(
        new_york.offset_at(utc(2026, 7, 1, 0, 0, 0)).utc_offset,
        -14_400
    );
    assert_eq!(
        Zone::named("UTC")
            .unwrap()
            .offset_at(utc(2026, 7, 1, 0, 0, 0))
            .utc_offset,
        0
    );
}

#[quench_test::test]
fn the_database_can_be_listed_and_queried_by_name() {
    let names: Vec<&str> = Zone::names().collect();
    assert!(names.len() > 500, "{}", names.len());
    assert!(names.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
    for expected in [
        "Europe/Warsaw",
        "America/New_York",
        "UTC",
        "US/Pacific",
        "Asia/Calcutta",
    ] {
        assert!(names.contains(&expected), "{expected}");
    }
    assert!(Zone::database_version().len() >= 5);

    let w: Zone = "Europe/Warsaw".parse().unwrap();
    assert_eq!(w, warsaw());
    assert_eq!(w.name(), "Europe/Warsaw");
    assert_eq!(w.to_string(), "Europe/Warsaw");
    assert_eq!(format!("{w:?}"), "Zone(Europe/Warsaw)");
    assert_ne!(w, Zone::utc());

    let error = Zone::named("Mars/Olympus").unwrap_err();
    assert_eq!(error.to_string(), "unknown time zone `Mars/Olympus`");
    assert!(
        "warsaw".parse::<Zone>().is_err(),
        "names are case sensitive"
    );
    assert!(Zone::named("").is_err());

    // The second lookup comes from the cache and agrees.
    assert_eq!(Zone::named("Europe/Warsaw").unwrap().data, w.data);
}

#[quench_test::test]
fn fixed_zones_never_change() {
    let utc_zone = Zone::utc();
    assert_eq!(utc_zone.name(), "UTC");
    assert_eq!(
        utc_zone.offset_at(utc(2026, 7, 1, 0, 0, 0)).abbreviation,
        "UTC"
    );
    let plus = Zone::fixed("Plus", 5 * 3600 + 30 * 60);
    let off = plus.offset_at(utc(2026, 7, 1, 0, 0, 0));
    assert_eq!(
        (off.utc_offset, off.abbreviation.as_str()),
        (19_800, "+0530")
    );
    assert_eq!(
        Zone::fixed("Minus", -3600)
            .offset_at(Time::from_unix(0))
            .abbreviation,
        "-0100"
    );
    assert_eq!(
        plus.resolve(&local(2026, 1, 1, 12, 0))
            .single()
            .map(|t| t.timestamp()),
        Some(utc(2026, 1, 1, 6, 30, 0).timestamp())
    );
}

/// A `TZif` v2 file with two types and one transition, and a POSIX rule after it.
fn tzif(rule: &str) -> Vec<u8> {
    let mut v = Vec::new();
    for pass in 0..2 {
        v.extend_from_slice(b"TZif2");
        v.extend_from_slice(&[0; 15]);
        let time_size = if pass == 0 { 4 } else { 8 };
        // isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt
        for n in [0u32, 0, 0, 1, 2, 8] {
            v.extend_from_slice(&n.to_be_bytes());
        }
        if time_size == 4 {
            v.extend_from_slice(&1_000_000i32.to_be_bytes());
        } else {
            v.extend_from_slice(&1_000_000i64.to_be_bytes());
        }
        v.push(1);
        v.extend_from_slice(&3600i32.to_be_bytes());
        v.extend_from_slice(&[0, 0]);
        v.extend_from_slice(&7200i32.to_be_bytes());
        v.extend_from_slice(&[1, 4]);
        v.extend_from_slice(b"AAA\0BBB\0");
    }
    v.extend_from_slice(b"\n");
    v.extend_from_slice(rule.as_bytes());
    v.extend_from_slice(b"\n");
    v
}

#[quench_test::test]
fn tzif_files_can_be_read() {
    let zone = Zone::from_tzif("Custom", &tzif("AAA-1BBB-2,M3.5.0,M10.5.0")).unwrap();
    assert_eq!(zone.name(), "Custom");
    // Before the transition: the first standard type.
    assert_eq!(zone.offset_at(Time::from_unix(0)).abbreviation, "AAA");
    // After it, type 1, then the rule takes over.
    assert_eq!(zone.data.transitions.len(), 1);
    let after = zone.offset_at(Time::from_unix(1_000_001));
    assert!(after.is_dst || after.abbreviation == "BBB" || after.abbreviation == "AAA");
    assert_eq!(zone.offset_at(utc(2026, 7, 1, 0, 0, 0)).abbreviation, "BBB");
    assert_eq!(zone.offset_at(utc(2026, 1, 1, 0, 0, 0)).abbreviation, "AAA");

    let no_rule = Zone::from_tzif("Plain", &tzif("")).unwrap();
    assert_eq!(
        no_rule.offset_at(utc(2026, 7, 1, 0, 0, 0)).abbreviation,
        "BBB",
        "the last transition's type stays"
    );
}

#[quench_test::test]
fn damaged_tzif_data_is_an_error() {
    let good = tzif("AAA-1");
    assert!(Zone::from_tzif("x", b"").is_err());
    assert!(Zone::from_tzif("x", b"NOPE0000000000000000000000000000000000000000000").is_err());
    assert!(Zone::from_tzif("x", &good[..50]).is_err());
    assert!(Zone::from_tzif("x", &good[..good.len() / 2]).is_err());
    let mut bad_rule = tzif("not a rule");
    assert!(Zone::from_tzif("x", &bad_rule).is_err());
    bad_rule.clear();
    let mut no_types = good;
    // typecnt of the first header -> 0
    no_types[36..40].copy_from_slice(&0u32.to_be_bytes());
    assert!(Zone::from_tzif("x", &no_types).is_err());
    assert_eq!(TzifError("x").to_string(), "invalid TZif data: x");
}

#[quench_test::test]
fn version_one_files_are_read_without_a_rule() {
    let mut v = Vec::new();
    v.extend_from_slice(b"TZif\0");
    v.extend_from_slice(&[0; 15]);
    for n in [0u32, 0, 0, 1, 2, 8] {
        v.extend_from_slice(&n.to_be_bytes());
    }
    v.extend_from_slice(&500i32.to_be_bytes());
    v.push(1);
    v.extend_from_slice(&0i32.to_be_bytes());
    v.extend_from_slice(&[0, 0]);
    v.extend_from_slice(&3600i32.to_be_bytes());
    v.extend_from_slice(&[0, 4]);
    v.extend_from_slice(b"GMT\0BST\0");
    let zone = Zone::from_tzif("v1", &v).unwrap();
    assert_eq!(zone.offset_at(Time::from_unix(100)).abbreviation, "GMT");
    assert_eq!(zone.offset_at(Time::from_unix(600)).utc_offset, 3600);
}

#[quench_test::test]
fn the_local_zone_comes_from_the_environment() {
    // `TZ` is process-wide, so everything about it is in one test.
    let saved = std::env::var_os("TZ");
    quench_core::env::set("TZ", "Asia/Tokyo");
    assert_eq!(Zone::local().name(), "Asia/Tokyo");
    quench_core::env::set("TZ", ":Europe/Warsaw");
    assert_eq!(Zone::local().name(), "Europe/Warsaw");
    quench_core::env::set("TZ", "XYZ-3");
    let rule_zone = Zone::local();
    assert_eq!(
        rule_zone.offset_at(utc(2026, 7, 1, 0, 0, 0)).utc_offset,
        10_800
    );
    quench_core::env::set("TZ", "Not/A_Zone");
    // Falls through to the machine's own setting, and always yields some zone.
    let _ = Zone::local().offset_at(utc(2026, 7, 1, 0, 0, 0));
    quench_core::env::set("TZ", "");
    let _ = Zone::local();
    match saved {
        Some(v) => quench_core::env::set("TZ", v),
        None => quench_core::env::remove("TZ"),
    }
}

#[quench_test::test]
fn invalid_calendar_fields_resolve_to_nothing() {
    let w = warsaw();
    let bad = Civil {
        month: 13,
        ..local(2026, 1, 1, 0, 0)
    };
    assert_eq!(w.resolve(&bad), Resolved::Gap);
}
