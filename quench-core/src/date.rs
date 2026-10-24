//! Calendar dates without a time or a zone: day arithmetic, month arithmetic, weekdays.

use std::fmt;
use std::str::FromStr;

use crate::time::{
    Civil, ParseTimeError, Weekday, civil_from_days, days_from_civil, days_in_month,
};

/// A day of the proleptic Gregorian calendar, counted from 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Date {
    days: i64,
}

impl Date {
    /// 1970-01-01.
    pub const EPOCH: Self = Self { days: 0 };

    /// `None` when the month or day does not exist (including 29 February in a common year).
    #[must_use]
    pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self {
            days: days_from_civil(i64::from(year), i64::from(month), i64::from(day)),
        })
    }

    /// How many days `month` (1-12) has in `year`.
    #[must_use]
    pub const fn days_in_month(year: i32, month: u32) -> u32 {
        days_in_month(year, month)
    }

    /// The date `days` after (negative: before) 1970-01-01.
    #[must_use]
    pub const fn from_days_since_epoch(days: i64) -> Self {
        Self { days }
    }

    #[must_use]
    pub const fn days_since_epoch(self) -> i64 {
        self.days
    }

    /// (year, month 1-12, day 1-31).
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn ymd(self) -> (i32, u32, u32) {
        let (y, m, d) = civil_from_days(self.days);
        (y as i32, m as u32, d as u32)
    }

    #[must_use]
    pub fn year(self) -> i32 {
        self.ymd().0
    }

    #[must_use]
    pub fn month(self) -> u32 {
        self.ymd().1
    }

    #[must_use]
    pub fn day(self) -> u32 {
        self.ymd().2
    }

    #[must_use]
    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday (index 3 counting from Monday).
        let index = usize::try_from((self.days + 3).rem_euclid(7)).unwrap_or(0);
        Weekday::ALL[index]
    }

    /// 1 for 1 January.
    #[must_use]
    pub fn day_of_year(self) -> u32 {
        let year = self.year();
        let start = days_from_civil(i64::from(year), 1, 1);
        u32::try_from(self.days - start + 1).unwrap_or(1)
    }

    /// The date `days` later (negative: earlier); `None` past the range of years this type can show.
    #[must_use]
    pub fn add_days(self, days: i64) -> Option<Self> {
        let moved = Self {
            days: self.days.checked_add(days)?,
        };
        (moved.year().abs() < 1_000_000).then_some(moved)
    }

    /// The date `months` later (negative: earlier), keeping the day of the month or, when that month is
    /// shorter, its last day (31 January plus one month is 28 or 29 February).
    #[must_use]
    pub fn add_months(self, months: i32) -> Option<Self> {
        let (year, month, day) = self.ymd();
        let index = i64::from(year) * 12 + i64::from(month) - 1 + i64::from(months);
        let year = i32::try_from(index.div_euclid(12)).ok()?;
        if year.abs() >= 1_000_000 {
            return None;
        }
        let month = u32::try_from(index.rem_euclid(12)).ok()? + 1;
        Self::from_ymd(year, month, day.min(days_in_month(year, month)))
    }

    /// Whole days from `earlier` to this date (negative when this one is first).
    #[must_use]
    pub const fn days_since(self, earlier: Self) -> i64 {
        self.days - earlier.days
    }

    /// This date at the given time of day, as calendar fields.
    #[must_use]
    pub fn at(self, hour: u32, minute: u32, second: u32) -> Civil {
        let (year, month, day) = self.ymd();
        Civil {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanosecond: 0,
        }
    }

    /// Reads `text` as `pattern`, which may use `%Y %m %d` and literal characters (`%Y-%m-%d`).
    ///
    /// # Errors
    ///
    /// [`ParseTimeError`] when the text does not match or names no real date.
    pub fn parse_from_str(text: &str, pattern: &str) -> Result<Self, ParseTimeError> {
        let bad = || ParseTimeError::new(format!("`{text}` is not a date like `{pattern}`"));
        let (mut year, mut month, mut day) = (None, None, None);
        let mut rest = text;
        let mut spec = pattern.chars();
        while let Some(c) = spec.next() {
            if c == '%' {
                let (slot, width) = match spec.next() {
                    Some('Y') => (&mut year, 4),
                    Some('m') => (&mut month, 2),
                    Some('d') => (&mut day, 2),
                    _ => return Err(bad()),
                };
                let digits = rest
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .count()
                    .min(width);
                if digits == 0 || (width == 4 && digits != 4) {
                    return Err(bad());
                }
                *slot = Some(rest[..digits].parse::<i32>().map_err(|_| bad())?);
                rest = &rest[digits..];
            } else {
                rest = rest.strip_prefix(c).ok_or_else(bad)?;
            }
        }
        if !rest.is_empty() {
            return Err(bad());
        }
        let (Some(year), Some(month), Some(day)) = (year, month, day) else {
            return Err(bad());
        };
        Self::from_ymd(
            year,
            u32::try_from(month).map_err(|_| bad())?,
            u32::try_from(day).map_err(|_| bad())?,
        )
        .ok_or_else(bad)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Date({self})")
    }
}

impl FromStr for Date {
    type Err = ParseTimeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse_from_str(text, "%Y-%m-%d")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> Date {
        Date::from_ymd(y, m, day).expect("a real date")
    }

    #[quench_test::test]
    fn construction_and_fields() {
        assert_eq!(Date::EPOCH.ymd(), (1970, 1, 1));
        assert_eq!(d(2024, 2, 29).ymd(), (2024, 2, 29));
        assert!(Date::from_ymd(2023, 2, 29).is_none());
        assert!(Date::from_ymd(2023, 13, 1).is_none());
        assert!(Date::from_ymd(2023, 4, 31).is_none());
        assert!(Date::from_ymd(2023, 0, 1).is_none());
        assert!(Date::from_ymd(2023, 1, 0).is_none());
        let date = d(2026, 10, 23);
        assert_eq!((date.year(), date.month(), date.day()), (2026, 10, 23));
        assert_eq!(date.weekday(), Weekday::Fri);
        assert_eq!(d(1970, 1, 1).weekday(), Weekday::Thu);
        assert_eq!(d(1969, 12, 31).weekday(), Weekday::Wed);
        assert_eq!(d(2024, 12, 31).day_of_year(), 366);
        assert_eq!(d(2025, 3, 1).day_of_year(), 60);
        assert_eq!(Date::from_days_since_epoch(1).days_since_epoch(), 1);
        assert_eq!(d(1900, 1, 1).days_since_epoch(), -25_567);
        assert_eq!(Date::days_in_month(2024, 2), 29);
        assert_eq!(Date::days_in_month(2023, 2), 28);
        assert_eq!(Date::days_in_month(2023, 12), 31);
        assert_eq!(Date::days_in_month(2023, 4), 30);
    }

    #[quench_test::test]
    fn day_arithmetic() {
        assert_eq!(d(2024, 2, 28).add_days(2), Some(d(2024, 3, 1)));
        assert_eq!(d(2023, 3, 1).add_days(-1), Some(d(2023, 2, 28)));
        assert_eq!(d(2026, 1, 1).add_days(365), Some(d(2027, 1, 1)));
        assert_eq!(d(2026, 10, 23).days_since(d(2026, 10, 1)), 22);
        assert_eq!(d(2026, 10, 1).days_since(d(2026, 10, 23)), -22);
        assert_eq!(d(2026, 1, 1).add_days(i64::MAX), None);
        assert_eq!(d(2026, 1, 1).add_days(i64::MAX / 2), None);
    }

    #[quench_test::test]
    fn month_arithmetic_clamps_to_the_end_of_the_month() {
        assert_eq!(d(2026, 1, 31).add_months(1), Some(d(2026, 2, 28)));
        assert_eq!(d(2024, 1, 31).add_months(1), Some(d(2024, 2, 29)));
        assert_eq!(d(2026, 1, 15).add_months(12), Some(d(2027, 1, 15)));
        assert_eq!(d(2026, 3, 31).add_months(-1), Some(d(2026, 2, 28)));
        assert_eq!(d(2026, 1, 1).add_months(-1), Some(d(2025, 12, 1)));
        assert_eq!(d(2026, 11, 30).add_months(3), Some(d(2027, 2, 28)));
        assert_eq!(d(2026, 5, 5).add_months(0), Some(d(2026, 5, 5)));
        assert_eq!(d(2026, 5, 5).add_months(-24_000), Some(d(26, 5, 5)));
        assert_eq!(d(2026, 5, 5).add_months(i32::MAX), None);
        assert_eq!(d(2026, 5, 5).add_months(i32::MIN), None);
    }

    #[quench_test::test]
    fn text_forms() {
        assert_eq!(d(2026, 9, 5).to_string(), "2026-09-05");
        assert_eq!(format!("{:?}", d(2026, 9, 5)), "Date(2026-09-05)");
        assert_eq!("2026-09-05".parse::<Date>(), Ok(d(2026, 9, 5)));
        assert!("2026-02-30".parse::<Date>().is_err());
        assert!(
            "2026-9-5".parse::<Date>().is_ok(),
            "single-digit month and day are fine"
        );
        assert!("26-09-05".parse::<Date>().is_err());
        assert!("2026-09-05x".parse::<Date>().is_err());
        assert!("2026/09/05".parse::<Date>().is_err());
        assert!("".parse::<Date>().is_err());
        assert_eq!(
            Date::parse_from_str("05.09.2026", "%d.%m.%Y"),
            Ok(d(2026, 9, 5))
        );
        assert_eq!(
            Date::parse_from_str("20260905", "%Y%m%d"),
            Ok(d(2026, 9, 5))
        );
        assert!(
            Date::parse_from_str("2026", "%Y").is_err(),
            "a pattern must name year, month and day"
        );
        assert!(Date::parse_from_str("2026-09-05", "%Y-%q-%d").is_err());
        assert!(Date::parse_from_str("2026-09-05", "%Y-%m-%").is_err());
    }

    #[quench_test::test]
    fn dates_and_civil_fields_convert() {
        let civil = d(2026, 10, 23).at(13, 45, 59);
        assert_eq!(
            (civil.hour, civil.minute, civil.second, civil.nanosecond),
            (13, 45, 59, 0)
        );
        assert_eq!(civil.date(), d(2026, 10, 23));
        assert_eq!(civil.weekday(), Weekday::Fri);
        let time = civil.to_time().expect("valid fields");
        assert_eq!(time.to_rfc3339(), "2026-10-23T13:45:59Z");
        assert_eq!(time.civil().date(), d(2026, 10, 23));
        let mut bad = civil;
        bad.hour = 24;
        assert!(bad.to_time().is_none());
        bad.hour = 1;
        bad.nanosecond = 1_000_000_000;
        assert!(bad.to_time().is_none());
        assert!(d(2026, 10, 23) < d(2026, 10, 24));
        assert_eq!(Date::default(), Date::EPOCH);
    }
}
