//! UTC time: instants, signed durations, RFC 3339 / RFC 2822 text.
//!
//! Deliberately UTC-only: every use in quench is a stored or logged instant,
//! never a calendar in a local zone. Both types hold plain nanosecond counts,
//! so ordering and arithmetic are exact and cannot overflow in practice.

use std::fmt::{self, Write as _};
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

const NANOS_PER_SEC: i128 = 1_000_000_000;
const SECS_PER_DAY: i64 = 86_400;

/// A signed span of time with nanosecond resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration {
    nanos: i128,
}

impl Duration {
    pub const ZERO: Self = Self { nanos: 0 };

    #[must_use]
    pub const fn nanoseconds(n: i64) -> Self {
        Self { nanos: n as i128 }
    }

    #[must_use]
    pub const fn microseconds(n: i64) -> Self {
        Self {
            nanos: n as i128 * 1_000,
        }
    }

    #[must_use]
    pub const fn milliseconds(n: i64) -> Self {
        Self {
            nanos: n as i128 * 1_000_000,
        }
    }

    #[must_use]
    pub const fn seconds(n: i64) -> Self {
        Self {
            nanos: n as i128 * NANOS_PER_SEC,
        }
    }

    #[must_use]
    pub const fn minutes(n: i64) -> Self {
        Self::seconds(n * 60)
    }

    #[must_use]
    pub const fn hours(n: i64) -> Self {
        Self::seconds(n * 3_600)
    }

    #[must_use]
    pub const fn days(n: i64) -> Self {
        Self::seconds(n * SECS_PER_DAY)
    }

    #[must_use]
    pub const fn weeks(n: i64) -> Self {
        Self::days(n * 7)
    }

    /// Whole seconds, rounded towards negative infinity.
    #[must_use]
    pub fn num_seconds(&self) -> i64 {
        saturate(self.nanos.div_euclid(NANOS_PER_SEC))
    }

    /// Whole milliseconds, rounded towards negative infinity.
    #[must_use]
    pub fn num_milliseconds(&self) -> i64 {
        saturate(self.nanos.div_euclid(1_000_000))
    }

    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.nanos < 0
    }

    /// As a [`std::time::Duration`]; `None` when negative.
    #[must_use]
    pub fn to_std(&self) -> Option<std::time::Duration> {
        let nanos = u128::try_from(self.nanos).ok()?;
        let secs = u64::try_from(nanos / NANOS_PER_SEC.unsigned_abs()).unwrap_or(u64::MAX);
        let sub = u32::try_from(nanos % NANOS_PER_SEC.unsigned_abs()).unwrap_or(0);
        Some(std::time::Duration::new(secs, sub))
    }

    #[must_use]
    pub fn from_std(d: std::time::Duration) -> Self {
        Self {
            nanos: i128::try_from(d.as_nanos()).unwrap_or(i128::MAX),
        }
    }
}

impl Add for Duration {
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        Self {
            nanos: self.nanos.saturating_add(rhs.nanos),
        }
    }
}

impl Sub for Duration {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self {
            nanos: self.nanos.saturating_sub(rhs.nanos),
        }
    }
}

impl Neg for Duration {
    type Output = Self;

    fn neg(self) -> Self {
        Self { nanos: -self.nanos }
    }
}

impl Mul<i64> for Duration {
    type Output = Self;

    fn mul(self, rhs: i64) -> Self {
        Self {
            nanos: self.nanos.saturating_mul(i128::from(rhs)),
        }
    }
}

impl fmt::Display for Duration {
    /// `-1h2m3.5s`-style: the largest units that are non-zero.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut rest = self.nanos.unsigned_abs();
        if self.nanos < 0 {
            f.write_str("-")?;
        }
        if rest == 0 {
            return f.write_str("0s");
        }
        let per_sec = NANOS_PER_SEC.unsigned_abs();
        let secs = rest / per_sec;
        rest %= per_sec;
        let (h, m, s) = (secs / 3_600, secs % 3_600 / 60, secs % 60);
        if h > 0 {
            write!(f, "{h}h")?;
        }
        if m > 0 {
            write!(f, "{m}m")?;
        }
        if s > 0 || rest > 0 {
            if rest == 0 {
                write!(f, "{s}s")?;
            } else {
                let frac = format!("{rest:09}");
                write!(f, "{s}.{}s", frac.trim_end_matches('0'))?;
            }
        }
        Ok(())
    }
}

/// Day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    const ALL: [Self; 7] = [
        Self::Mon,
        Self::Tue,
        Self::Wed,
        Self::Thu,
        Self::Fri,
        Self::Sat,
        Self::Sun,
    ];

    /// The three-letter English name (`Mon`, `Tue`, ...).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mon => "Mon",
            Self::Tue => "Tue",
            Self::Wed => "Wed",
            Self::Thu => "Thu",
            Self::Fri => "Fri",
            Self::Sat => "Sat",
            Self::Sun => "Sun",
        }
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A calendar date and wall-clock time, always UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    /// 1-12.
    pub month: u32,
    /// 1-31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub nanosecond: u32,
}

/// An instant in time, UTC, nanoseconds since 1970-01-01T00:00:00Z.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Time {
    nanos: i128,
}

/// Text that is not valid RFC 3339.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTimeError(String);

impl fmt::Display for ParseTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid RFC 3339 time: {}", self.0)
    }
}

impl std::error::Error for ParseTimeError {}

impl Time {
    pub const UNIX_EPOCH: Self = Self { nanos: 0 };

    /// The current time.
    ///
    /// On `wasm32` there is no system clock to read; the host installs one
    /// with `set_clock`.
    #[must_use]
    pub fn now() -> Self {
        Self {
            nanos: clock_nanos(),
        }
    }

    #[must_use]
    pub const fn from_unix(secs: i64) -> Self {
        Self {
            nanos: secs as i128 * NANOS_PER_SEC,
        }
    }

    #[must_use]
    pub const fn from_unix_millis(millis: i64) -> Self {
        Self {
            nanos: millis as i128 * 1_000_000,
        }
    }

    /// `nanos` beyond one second carry into the seconds.
    #[must_use]
    pub const fn from_unix_nanos(secs: i64, nanos: u32) -> Self {
        Self {
            nanos: secs as i128 * NANOS_PER_SEC + nanos as i128,
        }
    }

    /// Builds a time from calendar fields; `None` when a field is out of range
    /// (month 1-12, a day that exists in that month, hour < 24, minute < 60,
    /// second < 60).
    #[must_use]
    pub fn from_ymd_hms(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
        sec: u32,
    ) -> Option<Self> {
        if !(1..=12).contains(&month)
            || day == 0
            || day > days_in_month(year, month)
            || hour > 23
            || min > 59
            || sec > 59
        {
            return None;
        }
        let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
        let secs =
            days * SECS_PER_DAY + i64::from(hour) * 3_600 + i64::from(min) * 60 + i64::from(sec);
        Some(Self::from_unix(secs))
    }

    /// Whole seconds since the epoch, rounded towards negative infinity.
    #[must_use]
    pub fn timestamp(&self) -> i64 {
        saturate(self.nanos.div_euclid(NANOS_PER_SEC))
    }

    #[must_use]
    pub fn timestamp_millis(&self) -> i64 {
        saturate(self.nanos.div_euclid(1_000_000))
    }

    /// Nanoseconds past the whole second, `0..1_000_000_000`.
    #[must_use]
    pub fn subsec_nanos(&self) -> u32 {
        u32::try_from(self.nanos.rem_euclid(NANOS_PER_SEC)).unwrap_or(0)
    }

    /// The calendar fields.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn civil(&self) -> Civil {
        let secs = self.timestamp();
        let days = secs.div_euclid(SECS_PER_DAY);
        let rem = secs.rem_euclid(SECS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        Civil {
            year: year as i32,
            month: month as u32,
            day: day as u32,
            hour: (rem / 3_600) as u32,
            minute: (rem % 3_600 / 60) as u32,
            second: (rem % 60) as u32,
            nanosecond: self.subsec_nanos(),
        }
    }

    #[must_use]
    pub fn weekday(&self) -> Weekday {
        let days = self.timestamp().div_euclid(SECS_PER_DAY);
        // 1970-01-01 was a Thursday (index 3 counting from Monday).
        let index = usize::try_from((days + 3).rem_euclid(7)).unwrap_or(0);
        Weekday::ALL[index]
    }

    /// `2026-09-29T12:00:00Z`, with 3, 6 or 9 fractional digits only when the
    /// time has sub-second precision.
    #[must_use]
    pub fn to_rfc3339(&self) -> String {
        let c = self.civil();
        let mut out = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            c.year, c.month, c.day, c.hour, c.minute, c.second
        );
        let n = c.nanosecond;
        if n != 0 {
            let (value, width) = if n.is_multiple_of(1_000_000) {
                (n / 1_000_000, 3)
            } else if n.is_multiple_of(1_000) {
                (n / 1_000, 6)
            } else {
                (n, 9)
            };
            let _ = write!(out, ".{value:0width$}");
        }
        out.push('Z');
        out
    }

    /// `Tue, 29 Sep 2026 12:00:00 +0000` (the form mail `Date:` headers use).
    #[must_use]
    pub fn to_rfc2822(&self) -> String {
        let c = self.civil();
        format!(
            "{}, {} {} {:04} {:02}:{:02}:{:02} +0000",
            self.weekday().name(),
            c.day,
            MONTHS[(c.month - 1) as usize],
            c.year,
            c.hour,
            c.minute,
            c.second
        )
    }

    /// Parses RFC 3339: `YYYY-MM-DDTHH:MM:SS[.fraction](Z|±HH:MM)`. `T` may be
    /// lower case or a space, `Z` may be lower case; digits past nanoseconds
    /// are dropped; a leap second (`:60`) rolls into the next minute.
    pub fn parse_rfc3339(text: &str) -> Result<Self, ParseTimeError> {
        parse_rfc3339(text).ok_or_else(|| ParseTimeError(text.to_string()))
    }
}

impl FromStr for Time {
    type Err = ParseTimeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_rfc3339(text)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_rfc3339())
    }
}

impl fmt::Debug for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Time({})", self.to_rfc3339())
    }
}

impl Add<Duration> for Time {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self {
        Self {
            nanos: self.nanos.saturating_add(rhs.nanos),
        }
    }
}

impl Sub<Duration> for Time {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self {
        Self {
            nanos: self.nanos.saturating_sub(rhs.nanos),
        }
    }
}

impl Sub for Time {
    type Output = Duration;

    fn sub(self, rhs: Self) -> Duration {
        Duration {
            nanos: self.nanos.saturating_sub(rhs.nanos),
        }
    }
}

impl AddAssign<Duration> for Time {
    fn add_assign(&mut self, rhs: Duration) {
        *self = *self + rhs;
    }
}

impl SubAssign<Duration> for Time {
    fn sub_assign(&mut self, rhs: Duration) {
        *self = *self - rhs;
    }
}

fn saturate(n: i128) -> i64 {
    i64::try_from(n).unwrap_or(if n < 0 { i64::MIN } else { i64::MAX })
}

#[cfg(not(target_arch = "wasm32"))]
fn clock_nanos() -> i128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
        Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
    }
}

#[cfg(target_arch = "wasm32")]
static CLOCK: std::sync::OnceLock<fn() -> i128> = std::sync::OnceLock::new();

/// Installs the wall clock on `wasm32`, where `std` has none.
///
/// `now` must return nanoseconds since the Unix epoch. The first call wins.
#[cfg(target_arch = "wasm32")]
pub fn set_clock(now: fn() -> i128) {
    let _ = CLOCK.set(now);
}

#[cfg(target_arch = "wasm32")]
fn clock_nanos() -> i128 {
    let _ = (SystemTime::now, UNIX_EPOCH);
    CLOCK.get().map_or(0, |now| now())
}

const fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

const fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant's algorithm).
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Fixed-width ASCII digits at `pos`, as a number.
fn digits(bytes: &[u8], pos: usize, len: usize) -> Option<u32> {
    let slice = bytes.get(pos..pos + len)?;
    slice.iter().try_fold(0u32, |acc, &b| {
        b.is_ascii_digit().then(|| acc * 10 + u32::from(b - b'0'))
    })
}

fn parse_rfc3339(text: &str) -> Option<Time> {
    let b = text.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    if !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let year = i32::try_from(digits(b, 0, 4)?).ok()?;
    let (month, day) = (digits(b, 5, 2)?, digits(b, 8, 2)?);
    let (hour, min, sec) = (digits(b, 11, 2)?, digits(b, 14, 2)?, digits(b, 17, 2)?);
    if sec > 60 {
        return None;
    }
    // A leap second is read as :59 plus one second.
    let mut time = Time::from_ymd_hms(year, month, day, hour, min, sec.min(59))?;
    if sec == 60 {
        time += Duration::seconds(1);
    }

    let mut pos = 19;
    if b[pos] == b'.' {
        let start = pos + 1;
        let mut end = start;
        while b.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == start {
            return None;
        }
        // Keep nine digits at most, pad shorter fractions on the right.
        let mut nanos = 0i64;
        for i in 0..9 {
            let digit = if start + i < end {
                i64::from(b[start + i] - b'0')
            } else {
                0
            };
            nanos = nanos * 10 + digit;
        }
        time += Duration::nanoseconds(nanos);
        pos = end;
    }

    let offset_secs = match b.get(pos)? {
        b'Z' | b'z' if pos + 1 == b.len() => 0,
        sign @ (b'+' | b'-') if pos + 6 == b.len() && b[pos + 3] == b':' => {
            let (oh, om) = (digits(b, pos + 1, 2)?, digits(b, pos + 4, 2)?);
            if oh > 23 || om > 59 {
                return None;
            }
            let secs = i64::from(oh) * 3_600 + i64::from(om) * 60;
            if *sign == b'-' { -secs } else { secs }
        }
        _ => return None,
    };
    Some(time - Duration::seconds(offset_secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Time {
        Time::parse_rfc3339(s).unwrap()
    }

    #[test]
    fn epoch_and_unix_conversions() {
        assert_eq!(Time::UNIX_EPOCH.to_rfc3339(), "1970-01-01T00:00:00Z");
        assert_eq!(Time::from_unix(86_400).to_rfc3339(), "1970-01-02T00:00:00Z");
        assert_eq!(Time::from_unix(-1).to_rfc3339(), "1969-12-31T23:59:59Z");
        assert_eq!(Time::from_unix_millis(1_500).timestamp_millis(), 1_500);
        let x = Time::from_unix_nanos(10, 5);
        assert_eq!((x.timestamp(), x.subsec_nanos()), (10, 5));
        assert_eq!(Time::from_unix(-1).subsec_nanos(), 0);
        assert_eq!(Time::from_unix_millis(-1).timestamp(), -1);
        assert_eq!(Time::from_unix_millis(-1).subsec_nanos(), 999_000_000);
    }

    #[test]
    fn known_instants() {
        // 2026-09-29T12:00:00Z (the mail test fixture) and a leap-day date.
        let a = Time::from_ymd_hms(2026, 9, 29, 12, 0, 0).unwrap();
        assert_eq!(a.timestamp(), 1_790_683_200);
        assert_eq!(a.to_rfc2822(), "Tue, 29 Sep 2026 12:00:00 +0000");
        let b = Time::from_ymd_hms(2000, 2, 29, 23, 59, 59).unwrap();
        assert_eq!(b.timestamp(), 951_868_799);
        assert_eq!(b.weekday(), Weekday::Tue);
    }

    #[test]
    fn civil_round_trips_across_centuries() {
        for year in [
            1, 1600, 1900, 1969, 1970, 1999, 2000, 2024, 2100, 2400, 9999,
        ] {
            for (month, day) in [(1, 1), (2, 28), (3, 1), (12, 31)] {
                let time = Time::from_ymd_hms(year, month, day, 13, 14, 15).unwrap();
                let c = time.civil();
                assert_eq!(
                    (c.year, c.month, c.day, c.hour, c.minute, c.second),
                    (year, month, day, 13, 14, 15)
                );
            }
        }
    }

    #[test]
    fn rejects_impossible_calendar_fields() {
        for (y, mo, d, h, mi, s) in [
            (2026, 0, 1, 0, 0, 0),
            (2026, 13, 1, 0, 0, 0),
            (2026, 1, 0, 0, 0, 0),
            (2026, 4, 31, 0, 0, 0),
            (2026, 2, 29, 0, 0, 0),
            (1900, 2, 29, 0, 0, 0),
            (2026, 1, 1, 24, 0, 0),
            (2026, 1, 1, 0, 60, 0),
            (2026, 1, 1, 0, 0, 60),
        ] {
            assert!(
                Time::from_ymd_hms(y, mo, d, h, mi, s).is_none(),
                "{y}-{mo}-{d} {h}:{mi}:{s}"
            );
        }
        assert!(Time::from_ymd_hms(2000, 2, 29, 0, 0, 0).is_some());
        assert!(Time::from_ymd_hms(2024, 2, 29, 0, 0, 0).is_some());
        for month in [1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
            assert!(Time::from_ymd_hms(2026, month, 30, 0, 0, 0).is_some() || month == 2);
        }
    }

    #[test]
    fn weekdays_cycle() {
        let names: Vec<_> = (0..7)
            .map(|d| (Time::from_unix(d * 86_400)).weekday())
            .collect();
        assert_eq!(
            names,
            [
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sat,
                Weekday::Sun,
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed
            ]
        );
        assert_eq!(Time::from_unix(-86_400).weekday(), Weekday::Wed);
        for day in Weekday::ALL {
            assert_eq!(day.name().len(), 3);
        }
    }

    #[test]
    fn rfc3339_precision_is_automatic() {
        let base = Time::from_ymd_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(base.to_rfc3339(), "2026-01-02T03:04:05Z");
        assert_eq!(
            (base + Duration::milliseconds(120)).to_rfc3339(),
            "2026-01-02T03:04:05.120Z"
        );
        assert_eq!(
            (base + Duration::microseconds(1)).to_rfc3339(),
            "2026-01-02T03:04:05.000001Z"
        );
        assert_eq!(
            (base + Duration::nanoseconds(7)).to_rfc3339(),
            "2026-01-02T03:04:05.000000007Z"
        );
        assert_eq!(base.to_string(), base.to_rfc3339());
        assert_eq!(format!("{base:?}"), "Time(2026-01-02T03:04:05Z)");
    }

    #[test]
    fn parses_rfc3339_variants() {
        let want = Time::from_ymd_hms(2026, 9, 29, 12, 0, 0).unwrap();
        assert_eq!(t("2026-09-29T12:00:00Z"), want);
        assert_eq!(t("2026-09-29t12:00:00z"), want);
        assert_eq!(t("2026-09-29 12:00:00Z"), want);
        assert_eq!(t("2026-09-29T14:00:00+02:00"), want);
        assert_eq!(t("2026-09-29T09:30:00-02:30"), want);
        assert_eq!("2026-09-29T12:00:00Z".parse::<Time>().unwrap(), want);
        assert_eq!(
            t("2026-09-29T12:00:00.5Z"),
            want + Duration::milliseconds(500)
        );
        assert_eq!(
            t("2026-09-29T12:00:00.123456789123Z"),
            want + Duration::nanoseconds(123_456_789)
        );
        assert_eq!(t("2016-12-31T23:59:60Z"), t("2017-01-01T00:00:00Z"));
    }

    #[test]
    fn formatted_times_parse_back() {
        for secs in [
            -5_000_000_000_i64,
            -1,
            0,
            1,
            951_868_799,
            1_790_683_200,
            4_000_000_000,
        ] {
            for extra in [0, 1, 1_000, 1_000_000, 123_456_789] {
                let time = Time::from_unix_nanos(secs, extra);
                assert_eq!(t(&time.to_rfc3339()), time);
            }
        }
    }

    #[test]
    fn rejects_malformed_rfc3339() {
        for bad in [
            "",
            "2026-09-29",
            "2026-09-29T12:00:00",
            "2026-09-29T12:00:00Zjunk",
            "2026-09-29T12:00:00.Z",
            "2026-09-29T12:00:00+0200",
            "2026-09-29T12:00:00+02:0",
            "2026-09-29T12:00:00+24:00",
            "2026-09-29T12:00:00+02:60",
            "2026-09-29T12:00:61Z",
            "2026-13-29T12:00:00Z",
            "2026-02-30T12:00:00Z",
            "2026/09/29T12:00:00Z",
            "2026-09-29X12:00:00Z",
            "2026-09-29T12-00-00Z",
            "20x6-09-29T12:00:00Z",
            "2026-09-29T12:0a:00Z",
            "2026-09-29T12:00:00x",
        ] {
            let err = Time::parse_rfc3339(bad).unwrap_err();
            assert!(err.to_string().contains("invalid RFC 3339"), "{bad:?}");
        }
    }

    #[test]
    fn arithmetic() {
        let a = Time::from_unix(100);
        let mut b = a + Duration::seconds(30);
        assert_eq!(b.timestamp(), 130);
        assert_eq!((b - a).num_seconds(), 30);
        assert_eq!((a - b).num_seconds(), -30);
        b -= Duration::seconds(30);
        assert_eq!(b, a);
        b += Duration::minutes(1);
        assert_eq!(b.timestamp(), 160);
        assert_eq!((a - Duration::hours(1)).timestamp(), 100 - 3_600);
        assert!(a < b);
        assert_eq!(Time::default(), Time::UNIX_EPOCH);
    }

    #[test]
    fn duration_constructors_and_views() {
        assert_eq!(Duration::days(1), Duration::hours(24));
        assert_eq!(Duration::weeks(1), Duration::days(7));
        assert_eq!(Duration::minutes(2), Duration::seconds(120));
        assert_eq!(Duration::milliseconds(1_500).num_milliseconds(), 1_500);
        assert_eq!(Duration::milliseconds(1_500).num_seconds(), 1);
        assert_eq!(Duration::milliseconds(-1_500).num_seconds(), -2);
        assert!(Duration::seconds(-1).is_negative());
        assert!(!Duration::ZERO.is_negative());
        assert_eq!(-Duration::seconds(5), Duration::seconds(-5));
        assert_eq!(Duration::seconds(5) * 3, Duration::seconds(15));
        assert_eq!(
            Duration::seconds(5) - Duration::seconds(7),
            Duration::seconds(-2)
        );
        assert_eq!(
            Duration::seconds(5) + Duration::seconds(7),
            Duration::seconds(12)
        );
        assert_eq!(Duration::default(), Duration::ZERO);
    }

    #[test]
    fn duration_std_conversion() {
        let d = Duration::milliseconds(2_500);
        assert_eq!(d.to_std(), Some(std::time::Duration::from_millis(2_500)));
        assert_eq!(Duration::seconds(-1).to_std(), None);
        assert_eq!(
            Duration::from_std(std::time::Duration::from_secs(3)),
            Duration::seconds(3)
        );
    }

    #[test]
    fn duration_display() {
        assert_eq!(Duration::ZERO.to_string(), "0s");
        assert_eq!(Duration::seconds(90).to_string(), "1m30s");
        assert_eq!(Duration::seconds(3_723).to_string(), "1h2m3s");
        assert_eq!(Duration::milliseconds(1_500).to_string(), "1.5s");
        assert_eq!(Duration::milliseconds(-250).to_string(), "-0.25s");
        assert_eq!(Duration::hours(2).to_string(), "2h");
    }

    #[test]
    fn saturates_instead_of_wrapping() {
        let huge = Time { nanos: i128::MAX };
        assert_eq!(huge.timestamp(), i64::MAX);
        let tiny = Time { nanos: i128::MIN };
        assert_eq!(tiny.timestamp(), i64::MIN);
        assert_eq!((huge + Duration::days(1)).nanos, i128::MAX);
    }

    #[test]
    fn now_is_close_to_the_system_clock() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let now = Time::now().timestamp();
        assert!(now >= i64::try_from(before).unwrap());
        assert!(now - i64::try_from(before).unwrap() < 5);
    }
}
