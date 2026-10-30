---
title: Internals
parent: quench-tz
grand_parent: Crates
nav_order: 4
---

## Lookup model

A zone is a list of local-time types (offset, DST flag, abbreviation), a list of transitions `(instant, type)` and an optional POSIX rule for everything after the last transition. Dates far in the future are therefore right: the rule keeps applying.

## Packed database format (`data/zones.qtz`)

All integers are LEB128-style varints (7 bits per byte, little end first, high bit continues) unless stated. Signed values are zig-zag encoded (`(v << 1) ^ (v >> 63)`).

```text
magic        "QTZ1"                              4 bytes
version      varint length + UTF-8 text          e.g. "2026e"
name count   varint
names        name count x { varint length + UTF-8 name, varint zone index }   sorted by name
zone count   varint
lengths      zone count x varint byte length of that zone's body
bodies       the zone bodies back to back
```

Each zone body:

```text
rule text    varint length + UTF-8 POSIX rule (length 0: no rule)
type count   varint
types        type count x { zigzag varint utc_offset, 1 byte is_dst (0/1), varint length + UTF-8 abbreviation }
initial      varint index into types
trans count  varint
transitions  trans count x { zigzag varint delta from the previous instant (first: from 0), varint type index }
```

Identical bodies are stored once, which is how links and aliases cost nothing: 598 names map to 345 bodies in the shipped file. `Database::open` validates the magic and that body spans fit the file; bodies are decoded on demand (`Database::zone`) and cached by `Zone::named`.

## Rebuilding the database

```bash
cargo run -p quench-tz --example pack -- /usr/share/zoneinfo quench-tz/data/zones.qtz
```

`examples/pack.rs` walks the zoneinfo directory (skipping `posix/` and `right/`, `localtime` and `posixrules`; only files starting `TZif`; a file that fails to parse is reported and skipped), reads the version from the first line of `tzdata.zi` (`# version 2026e`), packs, writes the target and prints `<n> zones, version <v>, <bytes> bytes -> <target>`. Defaults: root `/usr/share/zoneinfo`, target `data/zones.qtz`. Rebuild and release when IANA publishes new rules.

## Concurrency

The database and the per-name cache are process statics (`OnceLock`, `Mutex<HashMap>`); a poisoned lock is ignored. `Zone` is cheap to clone (`Arc`).

## Verification

The crate's old documentation records a comparison against Python's `zoneinfo` over 265,825 instants and 448,465 local readings; the comparison script is not in the repository, so this is *unverified* here.
