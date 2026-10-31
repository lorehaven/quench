# quench-tz
IANA time zones: offsets, daylight saving and local-time resolution, with the zone database built in.

## What it is
`quench-tz` provides IANA time zones with the zone database built into the binary: the offset from UTC at any instant, daylight saving, and turning a local clock reading back into an instant. A container without `tzdata` behaves like a workstation.

It replaces `chrono-tz` and `chrono`'s `Local`. It builds on the `Time` and `Date` types of `quench-core`.

## Use cases
- Convert an instant to local calendar fields in a named zone.
- Turn a local clock reading into an instant, handling skipped and repeated times at DST changes.
- Get the machine's zone (from `TZ`, then `/etc/localtime`, then UTC).
- Use fixed offsets and UTC, or load a zone from TZif bytes.
- List the zone names in the embedded database.

## Example
```rust
use quench_core::time::Time;
use quench_tz::{Resolved, Zone};

let warsaw: Zone = "Europe/Warsaw".parse().unwrap();
let noon_utc = Time::from_ymd_hms(2026, 7, 1, 12, 0, 0).unwrap();
assert_eq!(warsaw.offset_at(noon_utc).utc_offset, 7200); // CEST
assert_eq!(warsaw.civil_at(noon_utc).hour, 14);

// 02:30 on the night the clocks go forward does not exist; 02:30 when they go back happens twice.
let gap = warsaw.resolve(&quench_core::date::Date::from_ymd(2026, 3, 29).unwrap().at(2, 30, 0));
assert!(matches!(gap, Resolved::Gap));
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-tz/
