# Quench TZ

`quench-tz` (crate `quench_tz`) replaces `chrono-tz` and `chrono`'s `Local`: IANA time zones with the database built in, so a container without `tzdata` behaves like a workstation. It depends only on `quench-core`.

```rust
use quench_core::{date::Date, time::Time};
use quench_tz::{Resolved, Zone};

let warsaw: Zone = "Europe/Warsaw".parse()?;
let noon_utc = Time::from_ymd_hms(2026, 7, 1, 12, 0, 0).unwrap();
warsaw.offset_at(noon_utc);            // 7200 s, daylight saving, "CEST"
warsaw.civil_at(noon_utc);             // 2026-07-01 14:00 local

// Local clock readings can be ambiguous or missing:
match warsaw.resolve(&Date::from_ymd(2026, 10, 25).unwrap().at(2, 30, 0)) {
    Resolved::Ambiguous { earlier, later } => { /* happens twice */ }
    Resolved::Single(t) => { /* one instant */ }
    Resolved::Gap => { /* the clocks skipped it */ }
}
```

## API

- **`Zone`**: `named("Europe/Warsaw")` (also `parse()`), `utc()`, `fixed(name, seconds)`, `local()`, `from_tzif(name, bytes)`, `names()` (all 598 names, sorted, aliases such as `US/Pacific` included), `database_version()`, `name()`. Names are case sensitive; an unknown one is `UnknownZone`.
- **`offset_at(Time) -> Offset`**: `utc_offset` (seconds), `is_dst`, `abbreviation` (`CET`, `+03`). **`civil_at(Time) -> Civil`**: the local calendar fields.
- **`resolve(&Civil) -> Resolved`**: `Single(Time)`, `Ambiguous { earlier, later }` (clocks turned back) or `Gap` (clocks turned forward). Same semantics as `chrono`'s `LocalResult`; `earliest()`, `latest()` and `single()` pick one.
- **`Zone::local()`** reads `TZ` (`Europe/Warsaw`, `:Europe/Warsaw`, a TZif path, or a POSIX rule such as `CET-1CEST,M3.5.0,M10.5.0/3`), then `/etc/localtime` (a link into `zoneinfo/`, or a TZif file), then falls back to UTC.
- Calendar arithmetic that does not depend on a zone lives in `quench_core::date::Date` (`add_days`, `add_months` clamping to the end of the month, `weekday`, `at(h, m, s)`), which is what recurring-event code combines with `resolve`.

## The database

`data/zones.qtz` (152 KB) holds every zone's local-time types, transitions and POSIX rule, with links stored once. It is built from a system `zoneinfo` directory by `cargo run -p quench-tz --example pack -- /usr/share/zoneinfo quench-tz/data/zones.qtz`, which reads the `tzdata.zi` header for the version; rebuild it and release when IANA publishes new rules. The current data is 2026e. After the last listed transition a zone follows its POSIX rule, so dates far in the future are right too.

## Testing

Unit tests next to the code (rule parsing, packing, TZif reading, gaps and repeats). Against Python's `zoneinfo` reading the same tzdata, all 598 zones agree on 265,825 instants (offset, daylight saving flag and abbreviation, including the seconds around every transition from 2000 to 2040) and 448,465 local readings (single, repeated or missing, with the exact instants).

[Home](../README.md)
