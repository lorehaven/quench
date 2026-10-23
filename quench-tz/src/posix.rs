//! POSIX `TZ` rules (`CET-1CEST,M3.5.0,M10.5.0/3`): what a zone does after its last listed transition.

use quench_core::date::Date;

/// When a daylight-saving period starts or ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    /// `Jn`: day 1-365, never counting 29 February.
    Julian1(u16),
    /// `n`: day 0-365, counting 29 February.
    Julian0(u16),
    /// `Mm.w.d`: weekday `d` (0 = Sunday) of week `w` (5 = last) of month `m`.
    Nth(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Change {
    day: Day,
    /// Seconds after local midnight (may be negative or past 24 hours).
    time: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Dst {
    abbreviation: String,
    utc_offset: i32,
    start: Change,
    end: Change,
}

/// A parsed rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    std_abbreviation: String,
    std_offset: i32,
    dst: Option<Dst>,
}

/// What a rule says about one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading<'a> {
    pub utc_offset: i32,
    pub is_dst: bool,
    pub abbreviation: &'a str,
}

struct Cursor<'a> {
    text: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.at;
        if self.eat(b'<') {
            let from = self.at;
            while self.peek()? != b'>' {
                self.at += 1;
            }
            let name = String::from_utf8_lossy(&self.text[from..self.at]).into_owned();
            self.at += 1;
            return (!name.is_empty()).then_some(name);
        }
        while self.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
            self.at += 1;
        }
        (self.at - start >= 3)
            .then(|| String::from_utf8_lossy(&self.text[start..self.at]).into_owned())
    }

    fn number(&mut self) -> Option<i32> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()?
            .parse()
            .ok()
    }

    /// `[+-]hh[:mm[:ss]]` in seconds.
    fn clock(&mut self) -> Option<i32> {
        let sign = if self.eat(b'-') {
            -1
        } else {
            self.eat(b'+');
            1
        };
        let hours = self.number()?;
        let mut seconds = hours.checked_mul(3600)?;
        if self.eat(b':') {
            seconds += self.number()? * 60;
            if self.eat(b':') {
                seconds += self.number()?;
            }
        }
        Some(sign * seconds)
    }

    fn change(&mut self) -> Option<Change> {
        let day = if self.eat(b'M') {
            let month = u8::try_from(self.number()?).ok()?;
            self.eat(b'.').then_some(())?;
            let week = u8::try_from(self.number()?).ok()?;
            self.eat(b'.').then_some(())?;
            let weekday = u8::try_from(self.number()?).ok()?;
            if !(1..=12).contains(&month) || !(1..=5).contains(&week) || weekday > 6 {
                return None;
            }
            Day::Nth(month, week, weekday)
        } else if self.eat(b'J') {
            let n = u16::try_from(self.number()?).ok()?;
            if !(1..=365).contains(&n) {
                return None;
            }
            Day::Julian1(n)
        } else {
            let n = u16::try_from(self.number()?).ok()?;
            if n > 365 {
                return None;
            }
            Day::Julian0(n)
        };
        let time = if self.eat(b'/') { self.clock()? } else { 7200 };
        Some(Change { day, time })
    }
}

impl Rule {
    /// `None` when the text is not a valid rule.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut c = Cursor {
            text: text.as_bytes(),
            at: 0,
        };
        let std_abbreviation = c.name()?;
        // POSIX counts hours *west* of Greenwich.
        let std_offset = -c.clock()?;
        if c.peek().is_none() {
            return Some(Self {
                std_abbreviation,
                std_offset,
                dst: None,
            });
        }
        let abbreviation = c.name()?;
        let utc_offset = if matches!(c.peek(), Some(b',') | None) {
            std_offset + 3600
        } else {
            -c.clock()?
        };
        // Without explicit rules the US ones apply (as glibc does).
        let (start, end) = if c.eat(b',') {
            let start = c.change()?;
            c.eat(b',').then_some(())?;
            (start, c.change()?)
        } else {
            (
                Change {
                    day: Day::Nth(3, 2, 0),
                    time: 7200,
                },
                Change {
                    day: Day::Nth(11, 1, 0),
                    time: 7200,
                },
            )
        };
        (c.peek().is_none()).then_some(Self {
            std_abbreviation,
            std_offset,
            dst: Some(Dst {
                abbreviation,
                utc_offset,
                start,
                end,
            }),
        })
    }

    /// The standard-time reading, for rules without daylight saving.
    fn standard(&self) -> Reading<'_> {
        Reading {
            utc_offset: self.std_offset,
            is_dst: false,
            abbreviation: &self.std_abbreviation,
        }
    }

    /// What applies at the instant `t` (seconds since the epoch).
    #[must_use]
    pub fn reading_at(&self, t: i64) -> Reading<'_> {
        let Some(dst) = &self.dst else {
            return self.standard();
        };
        let year = year_of(t + i64::from(self.std_offset));
        let start = instant(year, dst.start, self.std_offset);
        let end = instant(year, dst.end, dst.utc_offset);
        let in_dst = if start < end {
            start <= t && t < end
        } else {
            // The southern hemisphere: daylight saving spans the new year.
            !(end <= t && t < start)
        };
        if in_dst {
            Reading {
                utc_offset: dst.utc_offset,
                is_dst: true,
                abbreviation: &dst.abbreviation,
            }
        } else {
            self.standard()
        }
    }
}

fn year_of(local_seconds: i64) -> i32 {
    Date::from_days_since_epoch(local_seconds.div_euclid(86_400)).year()
}

/// The UTC instant of `change` in `year`, whose clock reads `offset` ahead of UTC just before it.
fn instant(year: i32, change: Change, offset: i32) -> i64 {
    let days = day_number(year, change.day);
    days * 86_400 + i64::from(change.time) - i64::from(offset)
}

/// Days since the epoch of the date `day` names in `year`.
fn day_number(year: i32, day: Day) -> i64 {
    let jan1 = Date::from_ymd(year, 1, 1).map_or(0, Date::days_since_epoch);
    match day {
        Day::Julian0(n) => jan1 + i64::from(n),
        Day::Julian1(n) => {
            let leap = Date::from_ymd(year, 2, 29).is_some();
            // Day 60 is 1 March in every year, so a leap year skips its 29 February.
            jan1 + i64::from(n) - 1 + i64::from(leap && n >= 60)
        }
        Day::Nth(month, week, weekday) => {
            let first = Date::from_ymd(year, u32::from(month), 1).unwrap_or_default();
            // 0 = Sunday, to match the rule; `Weekday` counts from Monday.
            let first_weekday = (first.days_since_epoch() + 4).rem_euclid(7);
            let mut day =
                (i64::from(weekday) - first_weekday).rem_euclid(7) + 1 + 7 * (i64::from(week) - 1);
            let length = i64::from(Date::days_in_month(year, u32::from(month)));
            while day > length {
                day -= 7;
            }
            first.days_since_epoch() + day - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::time::Time;

    fn at(rule: &Rule, rfc3339: &str) -> (i32, bool, String) {
        let t = Time::parse_rfc3339(rfc3339).expect("a time").timestamp();
        let r = rule.reading_at(t);
        (r.utc_offset, r.is_dst, r.abbreviation.to_string())
    }

    #[test]
    fn central_europe() {
        let rule = Rule::parse("CET-1CEST,M3.5.0,M10.5.0/3").expect("valid");
        assert_eq!(
            at(&rule, "2026-01-15T12:00:00Z"),
            (3600, false, "CET".into())
        );
        assert_eq!(
            at(&rule, "2026-07-15T12:00:00Z"),
            (7200, true, "CEST".into())
        );
        // Last Sunday of March 2026 is the 29th; the change is at 01:00 UTC.
        assert!(!at(&rule, "2026-03-29T00:59:59Z").1);
        assert!(at(&rule, "2026-03-29T01:00:00Z").1);
        // Last Sunday of October is the 25th; back at 01:00 UTC.
        assert!(at(&rule, "2026-10-25T00:59:59Z").1);
        assert!(!at(&rule, "2026-10-25T01:00:00Z").1);
    }

    #[test]
    fn the_southern_hemisphere_spans_the_new_year() {
        let rule = Rule::parse("AEST-10AEDT,M10.1.0,M4.1.0/3").expect("valid");
        assert_eq!(
            at(&rule, "2026-01-15T00:00:00Z"),
            (39_600, true, "AEDT".into())
        );
        assert_eq!(
            at(&rule, "2026-07-15T00:00:00Z"),
            (36_000, false, "AEST".into())
        );
        assert!(at(&rule, "2026-12-31T23:00:00Z").1);
        assert!(at(&rule, "2027-01-01T00:00:00Z").1);
    }

    #[test]
    fn rules_without_daylight_saving_and_odd_forms() {
        let fixed = Rule::parse("<+03>-3").expect("valid");
        assert_eq!(
            at(&fixed, "2026-07-15T00:00:00Z"),
            (10_800, false, "+03".into())
        );
        let utc = Rule::parse("UTC0").expect("valid");
        assert_eq!(at(&utc, "2026-07-15T00:00:00Z"), (0, false, "UTC".into()));
        let half = Rule::parse("<+0530>-5:30").expect("valid");
        assert_eq!(at(&half, "2026-07-15T00:00:00Z").0, 19_800);
        let lord_howe = Rule::parse("<+1030>-10:30<+11>-11,M10.1.0,M4.1.0").expect("valid");
        assert_eq!(
            at(&lord_howe, "2026-01-15T00:00:00Z"),
            (39_600, true, "+11".into())
        );
        let ireland = Rule::parse("IST-1GMT0,M10.5.0,M3.5.0/1").expect("valid");
        // Ireland's "daylight saving" is the winter GMT: the rule has a negative saving.
        assert_eq!(
            at(&ireland, "2026-01-15T00:00:00Z"),
            (0, true, "GMT".into())
        );
        assert_eq!(
            at(&ireland, "2026-07-15T00:00:00Z"),
            (3600, false, "IST".into())
        );
    }

    #[test]
    fn julian_days_and_negative_or_large_times() {
        let j = Rule::parse("EST5EDT,J60,J305").expect("valid");
        // J60 is always 1 March: in a leap year that is the 61st day.
        assert!(at(&j, "2024-03-01T12:00:00Z").1);
        assert!(!at(&j, "2024-02-29T12:00:00Z").1);
        let n = Rule::parse("EST5EDT,59,305").expect("valid");
        assert!(
            at(&n, "2024-02-29T12:00:00Z").1,
            "n counts 29 February (day 59)"
        );
        assert!(!at(&n, "2024-02-28T12:00:00Z").1);
        assert!(
            at(&n, "2023-03-01T12:00:00Z").1,
            "in a common year day 59 is 1 March"
        );
        let greenland = Rule::parse("<-02>2<-01>,M3.5.0/-1,M10.5.0/0").expect("valid");
        assert_eq!(at(&greenland, "2026-07-15T12:00:00Z").0, -3600);
        let long = Rule::parse("<+00>0<+01>-1,M3.5.0/26,M10.5.0/-2").expect("valid");
        assert_eq!(at(&long, "2026-07-15T12:00:00Z").0, 3600);
    }

    #[test]
    fn defaults_follow_the_us_rules() {
        let rule = Rule::parse("EST5EDT").expect("valid");
        assert_eq!(
            at(&rule, "2026-07-15T12:00:00Z"),
            (-14_400, true, "EDT".into())
        );
        assert_eq!(
            at(&rule, "2026-01-15T12:00:00Z"),
            (-18_000, false, "EST".into())
        );
    }

    #[test]
    fn bad_rules_are_rejected() {
        for bad in [
            "",
            "CET",
            "CET-1CEST,M3.5.0",
            "CET-1CEST,M13.1.0,M10.5.0",
            "CET-1CEST,M3.6.0,M10.5.0",
            "CET-1CEST,M3.5.7,M10.5.0",
            "AB-1",
            "<>-1",
            "<abc-1",
            "CET-1CEST,J0,J10",
            "CET-1CEST,J366,J10",
            "CET-1CEST,367,10",
            "CET-1CEST,M3.5.0,M10.5.0,",
            "CET-1CEST,M3.5.0,M10.5.0/x",
            "CET-1CEST,Mx.5.0,M10.5.0",
            "CET-1CEST,M3x5.0,M10.5.0",
            "CET-1CEST,M3.5x0,M10.5.0",
            "CET-1 junk",
        ] {
            assert!(Rule::parse(bad).is_none(), "{bad:?}");
        }
    }
}
