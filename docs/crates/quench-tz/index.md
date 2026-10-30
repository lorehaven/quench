---
title: quench-tz
parent: Crates
nav_order: 55
has_children: true
permalink: /crates/quench-tz/
---

`quench-tz` (library `quench_tz`) provides IANA time zones with the zone database built into the binary: the offset from UTC at any instant, daylight saving, and turning a local clock reading back into an instant. It replaces `chrono-tz` and `chrono`'s `Local`. A container without `tzdata` behaves like a workstation.

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 (workspace) |
| Features | none |
| Database | `data/zones.qtz`, 151,994 bytes, version `2026e`, 598 names (345 distinct rule sets), embedded with `include_bytes!` |
| Test harness | `harness = false` (library only); `src/tests.rs` under `cfg(test)` |

```toml
[dependencies]
quench-tz = { version = "0.1", registry = "ennor" }
```

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `Time`, `Duration`, `Civil`, `Date`, `env::var` |
| `quench-test` (dev) | harness |

## Minimal example (from the crate docs)

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

## Where to go next

* [Usage](usage.md), [API](api.md), [Errors](errors.md), [Internals](internals.md) (database format and how to rebuild it), [Testing](testing.md).
