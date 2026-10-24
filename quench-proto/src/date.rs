//! HTTP dates (RFC 9110 section 5.6.7): `Sun, 06 Nov 1994 08:49:37 GMT`.

use quench_core::time::Time;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats `time` as an IMF-fixdate.
#[must_use]
pub fn format(time: &Time) -> String {
    let c = time.civil();
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        time.weekday().name(),
        c.day,
        MONTHS[(c.month - 1) as usize],
        c.year,
        c.hour,
        c.minute,
        c.second
    )
}

/// The current time as an IMF-fixdate.
#[must_use]
pub fn now() -> String {
    format(&Time::now())
}

/// Parses an IMF-fixdate. The obsolete RFC 850 and asctime forms are not accepted.
#[must_use]
pub fn parse(text: &str) -> Option<Time> {
    let text = text.trim();
    let (_, rest) = text.split_once(", ")?;
    let mut parts = rest.split(' ');
    let day: u32 = parts.next()?.parse().ok()?;
    let month = parts.next()?;
    let month = u32::try_from(MONTHS.iter().position(|m| m.eq_ignore_ascii_case(month))?).ok()? + 1;
    let year: i32 = parts.next()?.parse().ok()?;
    let clock = parts.next()?;
    if parts.next()? != "GMT" || parts.next().is_some() {
        return None;
    }
    let mut hms = clock.split(':');
    let hour: u32 = hms.next()?.parse().ok()?;
    let minute: u32 = hms.next()?.parse().ok()?;
    let second: u32 = hms.next()?.parse().ok()?;
    if hms.next().is_some() {
        return None;
    }
    Time::from_ymd_hms(year, month, day, hour, minute, second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn formats_the_rfc_example() {
        let time = Time::from_ymd_hms(1994, 11, 6, 8, 49, 37).unwrap();
        assert_eq!(format(&time), "Sun, 06 Nov 1994 08:49:37 GMT");
    }

    #[quench_test::test]
    fn parses_what_it_formats() {
        let time = parse("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert_eq!(time.timestamp(), 784_111_777);
        assert_eq!(format(&time), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert!(parse("  Tue, 01 Jan 2030 00:00:00 GMT ").is_some());
    }

    #[quench_test::test]
    fn rejects_other_forms() {
        for bad in [
            "",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 Nov 1994 08:49:37 GMT extra",
            "Sun, 06 Foo 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 08:49 GMT",
            "Sun, 06 Nov 1994 08:49:37:11 GMT",
            "Sun, 32 Nov 1994 08:49:37 GMT",
            "Sun, xx Nov 1994 08:49:37 GMT",
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    #[quench_test::test]
    fn now_is_parseable() {
        assert!(parse(&now()).is_some());
    }
}
