---
title: Testing
parent: quench-archive
nav_order: 5
---

* Custom harness: `[lib] harness = false`, `quench_test::main!();`; integration tests `tests/tar.rs`, `tests/zip.rs`, `tests/zstd.rs` (each `harness = false`) plus unit tests inside the modules (for example `gzip.rs`: concatenated members, optional header fields).
* `tests/tar.rs`: ustar fixtures, GNU long names, pax headers, writer/reader agreement, base-256 numbers, unusable paths refused, damaged archives (cut and flipped bytes) are errors, a header declaring 2^40 bytes yields `Truncated` instead of allocating, pax overriding header fields.
* `tests/zip.rs`: deflated and stored entries, an archive comment, zip64 extra fields and zip64 end record (built by hand), writer/reader agreement, limits and damage, encryption and unknown methods are `Unsupported`.
* `tests/zstd.rs`: streams from the reference encoder (fixtures), encoder round trips, incompressible data cost, determinism, concatenated and skippable frames, hand-built frames, limits and damage.
* Run: `cargo test -p quench-archive`.
* *Unverified here:* the old page's claims of cross-checks against Python `zlib`, `zstd` levels 1-22, GNU tar, Info-ZIP and size comparisons with `zstd -3/-19` and `gzip -9` (for 2 MB of Rust source: 513 KB, 485 KB, 373 KB, 469 KB) are not reproducible from this repository's tests.

Testing code that uses the crate: all writers are deterministic (fixed mtime 0, DOS date 1980-01-01, OS byte 255), so archives can be compared byte for byte. Use a small `limit` to test the `Error::Limit` path.
