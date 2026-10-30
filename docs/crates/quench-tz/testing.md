---
title: Testing
parent: quench-tz
grand_parent: Crates
nav_order: 5
---

Tests are in `src/tests.rs` (`#[cfg(test)]`, `quench_test::test`) with `harness = false` on the library: offsets following daylight saving, local fields and the reverse direction, gaps and repeated hours, unusual zones, listing and querying names (more than 500 names, sorted, `US/Pacific` and `Asia/Calcutta` present, case sensitivity, cache), fixed zones, reading TZif (with a built-in TZif generator for rules), damaged TZif data, version-1 files, the local zone from `TZ` (one test, since the variable is process-wide), and invalid calendar fields resolving to nothing. Unit tests for POSIX rule parsing and packing sit in `posix.rs` and `pack.rs`.

```bash
cargo test -p quench-tz
```

`cargo test --offline -p quench-tz`: 20 unit tests pass (11 in `tests.rs`, 6 in `posix.rs`, 3 in `pack.rs`) and the crate-level doctest passes.
