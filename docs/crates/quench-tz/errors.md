---
title: Errors
parent: quench-tz
grand_parent: Crates
nav_order: 3
---

| Type | Returned by | Display |
|---|---|---|
| `UnknownZone(String)` | `Zone::named`, `str::parse::<Zone>()` | ``unknown time zone `<name>` `` |
| `TzifError(&'static str)` | `Zone::from_tzif`, `ZoneData::from_tzif` | `invalid TZif data: <reason>` (reasons: `missing TZif magic`, `truncated header`, `truncated data`, `no local time types`, `transitions out of order`, `transition to an unknown type`, `abbreviation out of range`, `bad time`, `unreadable POSIX rule`) |

`Zone::local()`, `Rule::parse` and `Database::open` do not return error types: `local()` always yields some zone (UTC at worst); the other two return `Option`. `Zone::resolve` reports "no such instant" as `Resolved::Gap`.

The built-in database is opened lazily with `.expect("the built-in zone database is valid")`; a corrupt embedded file would panic on first use.
