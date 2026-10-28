use quench_android::call;
use quench_android::ops::next_local;
use quench_core::time::Time;
use quench_tz::Zone;

fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    Time::from_ymd_hms(y, mo, d, h, mi, 0)
        .unwrap()
        .timestamp_millis()
}

fn local(zone: &str, ms: i64) -> String {
    let c = zone
        .parse::<Zone>()
        .unwrap()
        .civil_at(Time::from_unix_millis(ms));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        c.year, c.month, c.day, c.hour, c.minute
    )
}

#[quench_test::test]
fn the_next_reading_is_today_if_it_has_not_passed_and_tomorrow_if_it_has() {
    // 2026-10-27 is winter time in Warsaw (UTC+1): 08:00 there is 07:00 UTC.
    let before = utc(2026, 10, 27, 6, 0);
    assert_eq!(
        local(
            "Europe/Warsaw",
            next_local("Europe/Warsaw", 8, 0, before).unwrap()
        ),
        "2026-10-27 08:00"
    );
    let after = utc(2026, 10, 27, 7, 0);
    assert_eq!(
        local(
            "Europe/Warsaw",
            next_local("Europe/Warsaw", 8, 0, after).unwrap()
        ),
        "2026-10-28 08:00"
    );
    let well_after = utc(2026, 10, 27, 20, 0);
    assert_eq!(
        next_local("Europe/Warsaw", 8, 0, well_after).unwrap(),
        utc(2026, 10, 28, 7, 0)
    );
}

#[quench_test::test]
fn a_daily_reminder_stays_at_the_same_clock_time_through_both_changes() {
    let mut at = utc(2026, 1, 1, 0, 0);
    let mut last = at;
    for day in 0..400 {
        at = next_local("Europe/Warsaw", 8, 0, at).unwrap();
        assert!(at > last, "day {day}: the sequence goes forward");
        assert_eq!(
            local("Europe/Warsaw", at).split(' ').nth(1),
            Some("08:00"),
            "day {day}"
        );
        last = at;
    }
}

#[quench_test::test]
fn the_zone_decides_not_the_device() {
    let after = utc(2026, 7, 1, 0, 0);
    let tokyo = next_local("Asia/Tokyo", 9, 30, after).unwrap();
    assert_eq!(local("Asia/Tokyo", tokyo), "2026-07-01 09:30");
    assert_eq!(tokyo, utc(2026, 7, 1, 0, 30));
    let kolkata = next_local("Asia/Kolkata", 9, 30, after).unwrap();
    assert_eq!(kolkata, utc(2026, 7, 1, 4, 0), "+5:30");
}

#[quench_test::test]
fn a_reading_the_clocks_skip_fires_when_the_same_time_has_passed() {
    // 2026-03-29: at 02:00 the clocks in Warsaw go to 03:00, so 02:30 never happens.
    let after = utc(2026, 3, 29, 0, 0);
    let at = next_local("Europe/Warsaw", 2, 30, after).unwrap();
    assert_eq!(local("Europe/Warsaw", at), "2026-03-29 03:30");
    // The next day it is the ordinary 02:30.
    let next = next_local("Europe/Warsaw", 2, 30, at).unwrap();
    assert_eq!(local("Europe/Warsaw", next), "2026-03-30 02:30");
}

#[quench_test::test]
fn a_reading_that_happens_twice_fires_the_first_time() {
    // 2026-10-25: at 03:00 the clocks go back to 02:00, so 02:30 happens at +2 and again at +1.
    let after = utc(2026, 10, 24, 22, 0);
    let first = next_local("Europe/Warsaw", 2, 30, after).unwrap();
    assert_eq!(first, utc(2026, 10, 25, 0, 30), "CEST, UTC+2");
    // Asked again after the first one, it is the second time round (an hour later).
    let second = next_local("Europe/Warsaw", 2, 30, first).unwrap();
    assert_eq!(second, utc(2026, 10, 25, 1, 30), "CET, UTC+1");
}

#[quench_test::test]
fn mistakes_are_described() {
    assert!(
        next_local("Mars/Olympus", 8, 0, 0)
            .unwrap_err()
            .contains("no time zone")
    );
    assert!(
        next_local("UTC", 24, 0, 0)
            .unwrap_err()
            .contains("not a time of day")
    );
    assert!(next_local("UTC", 8, 60, 0).is_err());
}

#[quench_test::test]
fn the_operation_reads_json_and_answers_json() {
    let ok = call("tz.next", br#"{"zone":"UTC","time":"08:00","after_ms":0}"#).unwrap();
    assert_eq!(String::from_utf8(ok).unwrap(), r#"{"at_ms":28800000}"#);
    for (input, want) in [
        (&br#"{"time":"08:00","after_ms":0}"#[..], "zone"),
        (br#"{"zone":"UTC","after_ms":0}"#, "time"),
        (br#"{"zone":"UTC","time":"08:00"}"#, "after_ms"),
        (br#"{"zone":"UTC","time":"0800","after_ms":0}"#, "HH:MM"),
        (br#"{"zone":"UTC","time":"ab:cd","after_ms":0}"#, "HH:MM"),
        (b"not json", ""),
        (&[0xff, 0xfe][..], ""),
    ] {
        let e = call("tz.next", input).unwrap_err();
        assert!(e.contains(want), "{e}");
    }
}
