//! The operations that come with the crate. They are what several plugins share; an app adds its own
//! with [`register`](crate::register).

use quench_core::date::Date;
use quench_core::time::Time;
use quench_serde::{Value, json};
use quench_tz::{Resolved, Zone};

use crate::register;

pub(crate) fn register_all() {
    register("core.version", |_| {
        Ok(env!("CARGO_PKG_VERSION").as_bytes().to_vec())
    });
    register("core.echo", |input| Ok(input.to_vec()));
    register("tz.next", tz_next);
}

/// The first instant after `after_ms` (milliseconds since the epoch) when the clock in `zone` reads
/// `hour:minute`.
///
/// This is what a daily reminder needs, and what `AlarmManager` cannot do: it counts elapsed time,
/// so a fixed interval drifts by an hour when the clocks change.
///
/// A reading that happens twice (the clocks go back) fires the first time it is reached; one that
/// never happens (they go forward) fires after the same time has passed since the day before, which
/// is an hour later on the clock.
///
/// # Errors
/// An unknown zone or a time that is not a clock reading.
pub fn next_local(zone: &str, hour: u32, minute: u32, after_ms: i64) -> Result<i64, String> {
    if hour > 23 || minute > 59 {
        return Err(format!("{hour:02}:{minute:02} is not a time of day"));
    }
    let zone: Zone = zone.parse().map_err(|_| format!("no time zone `{zone}`"))?;
    let after = Time::from_unix_millis(after_ms);
    let today = zone.civil_at(after);
    let first =
        Date::from_ymd(today.year, today.month, today.day).ok_or("the date is out of range")?;
    // A day later at most, but the clocks may move: look a few days ahead before giving up.
    for days in 0..4 {
        let date = first.add_days(days).ok_or("the date is out of range")?;
        let reading = date.at(hour, minute, 0);
        let candidates = match zone.resolve(&reading) {
            Resolved::Single(t) => vec![t],
            Resolved::Ambiguous { earlier, later } => vec![earlier, later],
            Resolved::Gap => {
                // Taken as if the clock had not moved: the offset of the evening before.
                let as_utc =
                    Time::from_ymd_hms(reading.year, reading.month, reading.day, hour, minute, 0)
                        .ok_or("the date is out of range")?
                        .timestamp_millis();
                let before = zone
                    .offset_at(Time::from_unix_millis(as_utc - 12 * 3_600_000))
                    .utc_offset;
                vec![Time::from_unix_millis(as_utc - i64::from(before) * 1000)]
            }
        };
        if let Some(t) = candidates
            .into_iter()
            .find(|t| t.timestamp_millis() > after_ms)
        {
            return Ok(t.timestamp_millis());
        }
    }
    Err("no occurrence in the next days".to_string())
}

/// `tz.next`: `{"zone": "Europe/Warsaw", "time": "08:00", "after_ms": 1793000000000}` answers
/// `{"at_ms": ...}`.
fn tz_next(input: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(input).map_err(|e| e.to_string())?;
    let request: Value = quench_serde::json::from_str(text).map_err(|e| e.to_string())?;
    let zone = request
        .get("zone")
        .and_then(Value::as_str)
        .ok_or("`zone` is missing")?;
    let time = request
        .get("time")
        .and_then(Value::as_str)
        .ok_or("`time` is missing")?;
    let after = request
        .get("after_ms")
        .and_then(Value::as_i64)
        .ok_or("`after_ms` is missing")?;
    let (h, m) = time.split_once(':').ok_or("`time` is HH:MM")?;
    let hour: u32 = h.parse().map_err(|_| "`time` is HH:MM")?;
    let minute: u32 = m.parse().map_err(|_| "`time` is HH:MM")?;
    let at = next_local(zone, hour, minute, after)?;
    Ok(quench_serde::json::to_string(&json!({ "at_ms": at })).into_bytes())
}
