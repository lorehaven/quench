---
title: API
parent: quench-tz
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_tz` (`src/lib.rs`, plus the re-exports from the private modules `pack`, `posix`, `zone`).

## Root re-exports

```rust
pub use pack::{Database, pack};
pub use posix::Rule;
pub use zone::{LocalType, Reading, TzifError, ZoneData};
```

## `Zone`

```rust
#[derive(Clone)]
pub struct Zone { /* name: Arc<str>, data: Arc<ZoneData> */ }
```

`Display` prints the name; `Debug` prints `Zone(<name>)`; `PartialEq`/`Eq` compare **names only**; `FromStr` (`Err = UnknownZone`) is `named`. Cheap to clone; `Send + Sync`.

| Method | Signature | Behaviour |
|---|---|---|
| `utc` | `#[must_use] pub fn utc() -> Self` | `fixed("UTC", 0)` |
| `fixed` | `#[must_use] pub fn fixed(name: &str, utc_offset: i32) -> Self` | never changes; abbreviation `UTC` for 0, else `+HHMM` / `-HHMM` |
| `named` | `pub fn named(name: &str) -> Result<Self, UnknownZone>` | exact (case-sensitive) name from the built-in database; zone data is cached per process |
| `from_tzif` | `pub fn from_tzif(name: &str, bytes: &[u8]) -> Result<Self, TzifError>` | reads TZif (RFC 8536) bytes |
| `local` | `#[must_use] pub fn local() -> Self` | `TZ` (leading `:` removed; a database name, else a POSIX rule such as `XYZ-3` or `CET-1CEST,M3.5.0,M10.5.0/3`, else a path to a TZif file); then `/etc/localtime` (a symlink containing `zoneinfo/`, else the file as TZif named `Local`); else UTC |
| `names` | `pub fn names() -> impl Iterator<Item = &'static str>` | all 598 names, sorted, aliases included |
| `database_version` | `#[must_use] pub fn database_version() -> &'static str` | `2026e` |
| `name` | `#[must_use] pub fn name(&self) -> &str` | |
| `offset_at` | `#[must_use] pub fn offset_at(&self, t: Time) -> Offset` | offset in force at the instant |
| `civil_at` | `#[must_use] pub fn civil_at(&self, t: Time) -> Civil` | the local calendar fields (UTC time plus offset) |
| `resolve` | `#[must_use] pub fn resolve(&self, local: &Civil) -> Resolved` | which instants the reading names; sub-second fields carry over |

`resolve` algorithm: read `local` as if UTC, collect the distinct offsets in force within two days either side, and keep each candidate instant `as_utc - offset` whose own offset equals the candidate. None gives `Gap` (also when `local.to_time()` is `None`), one gives `Single`, more give `Ambiguous { earlier: first, later: last }`.

## `UnknownZone`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownZone(pub String);
```

`Display`: ``unknown time zone `<name>` ``. Implements `Error`.

## `Offset`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offset { pub utc_offset: i32 /* seconds ahead of UTC */, pub is_dst: bool, pub abbreviation: String /* CET, CEST, +03 */ }
```

## `Resolved`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Single(Time),
    Ambiguous { earlier: Time, later: Time },   // clocks turned back
    Gap,                                        // clocks turned forward over it
}
#[must_use] pub const fn single(self) -> Option<Time>     // only for Single
#[must_use] pub const fn earliest(self) -> Option<Time>   // Single, or Ambiguous.earlier
#[must_use] pub const fn latest(self) -> Option<Time>     // Single, or Ambiguous.later
```

## `ZoneData`, `LocalType`, `Reading`, `TzifError`

```rust
pub struct LocalType { pub utc_offset: i32, pub is_dst: bool, pub abbreviation: String }   // Debug, Clone, PartialEq, Eq
pub struct ZoneData {
    pub types: Vec<LocalType>,
    pub initial: usize,                    // index into types for times before the first transition
    pub transitions: Vec<(i64, usize)>,    // (instant in seconds since the epoch, index into types), ascending
    pub rule: Option<Rule>,                // what applies after the last transition
    pub rule_text: String,                 // text of `rule`, kept for packing
}                                          // Debug, Clone, PartialEq, Eq
pub struct Reading<'a> { pub utc_offset: i32, pub is_dst: bool, pub abbreviation: &'a str }   // Debug, Clone, Copy, PartialEq, Eq
pub struct TzifError(pub &'static str);    // Debug, Clone, PartialEq, Eq; Display: "invalid TZif data: {0}"; Error
impl ZoneData {
    #[must_use] pub fn fixed(utc_offset: i32, abbreviation: &str) -> Self
    #[must_use] pub fn reading_at(&self, t: i64) -> Reading<'_>
    pub fn from_tzif(bytes: &[u8]) -> Result<Self, TzifError>
}
```

* `reading_at(t)`: the first transition after `t` is found by binary search; after the last transition the `rule` (if any) decides; before the first, `initial`. A bad type index falls back to the first type, or `UTC`.
* `from_tzif`: uses the 64-bit block of version 2+ files (else the 32-bit block) and the footer POSIX rule when present. Error texts: `missing TZif magic`, `truncated header`, `truncated data`, `no local time types`, `transitions out of order`, `transition to an unknown type`, `abbreviation out of range`, `bad time`, `unreadable POSIX rule`.

## `Rule`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule { /* std_abbreviation, std_offset, dst: Option<Dst> */ }
impl Rule {
    #[must_use] pub fn parse(text: &str) -> Option<Self>
    #[must_use] pub fn reading_at(&self, t: i64) -> /* posix::Reading<'_> (not nameable) */
}
```

A parsed POSIX `TZ` rule. `parse` returns `None` for invalid text. Day forms: `Jn` (1-365 without 29 February), `n` (0-365 with it), `Mm.w.d` (weekday `d`, 0 = Sunday, of week `w`, 5 = last, of month `m`); times may be negative or beyond 24 hours. `reading_at` returns the private module's own `Reading` struct (same fields as `Reading` above), which cannot be named from outside the crate.

## `Database` and `pack`

```rust
#[derive(Debug)]
pub struct Database { /* version, names, spans, bytes */ }
impl Database {
    #[must_use] pub fn open(bytes: &'static [u8]) -> Option<Self>
    #[must_use] pub const fn version(&self) -> &'static str
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_
    #[must_use] pub fn canonical(&self, name: &str) -> Option<&'static str>
    #[must_use] pub fn zone(&self, name: &str) -> Option<ZoneData>
}
#[must_use] pub fn pack(version: &str, zones: &[(String, ZoneData)]) -> Vec<u8>
```

`open` returns `None` for bytes that are not a packed database (bad magic, truncated, spans out of range). `canonical` and `zone` look names up by binary search (exact spelling). `pack` packs zones and the version text; identical rule sets (links) are stored once. Format: [Internals](internals.md).
