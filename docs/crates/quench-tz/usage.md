---
title: Usage
parent: quench-tz
grand_parent: Crates
nav_order: 1
---

## 1. Offsets and local fields

```rust
let tokyo = quench_tz::Zone::named("Asia/Tokyo")?;
let t = quench_core::time::Time::from_ymd_hms(2026, 1, 1, 0, 0, 0).unwrap();
let offset = tokyo.offset_at(t);       // utc_offset 32400, is_dst false, abbreviation "JST"
let local = tokyo.civil_at(t);         // 2026-01-01 09:00 local
```

Names are case sensitive (`warsaw` is an error).

## 2. A local reading to an instant

```rust
use quench_core::date::Date;
use quench_tz::Resolved;
let warsaw = quench_tz::Zone::named("Europe/Warsaw")?;
let reading = Date::from_ymd(2026, 10, 25).unwrap().at(2, 30, 0);
match warsaw.resolve(&reading) {
    Resolved::Single(t) => println!("{t}"),
    Resolved::Ambiguous { earlier, later } => println!("twice: {earlier} and {later}"),
    Resolved::Gap => println!("skipped by the clocks"),
}
```

`earliest()`, `latest()` and `single()` collapse a `Resolved` to an `Option<Time>`. Pitfall: `Gap` also results from an invalid calendar field.

## 3. The machine's zone

```rust
let here = quench_tz::Zone::local();    // TZ, then /etc/localtime, then UTC
```

## 4. Fixed offsets and UTC

```rust
let utc = quench_tz::Zone::utc();
let plus_five_thirty = quench_tz::Zone::fixed("IST-fixed", 19_800); // abbreviation "+0530"
```

## 5. Zones from TZif bytes

```rust
let bytes = std::fs::read("/usr/share/zoneinfo/Europe/Warsaw")?;
let zone = quench_tz::Zone::from_tzif("Europe/Warsaw", &bytes)?;
```

## 6. List the database

```rust
for name in quench_tz::Zone::names().filter(|n| n.starts_with("Europe/")) { println!("{name}"); }
println!("{}", quench_tz::Zone::database_version());   // "2026e"
```
