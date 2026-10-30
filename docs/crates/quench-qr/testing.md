---
title: Testing
parent: quench-qr
nav_order: 4
---

* `tests/qr.rs` (custom harness, `harness = false`, `quench_test::main!()`): a golden symbol for "Hello, World" at level M compared module by module with `qrencode -8 -l M`'s output; the smallest-version selection and the exact limits (2953 bytes at L, 1273 at H, `Error::TooLong { len, max: 1273 }`); structural invariants for every (version, level): finder/timing/alignment patterns, both format-information copies re-derived with a BCH routine, version information for version 7 and up, matching data length; determinism; modules outside the symbol are light; the SVG shape; and a payload of exactly `capacity` bytes for each of the 160 (version, level) pairs, which must select that version.
* Unit tests in `src/layout.rs` check capacities against ISO 18004 table 7 for several versions.
* Run: `cargo test -p quench-qr`.
* The `dump` example (`cargo run -p quench-qr --example dump -- <L|M|Q|H> <text>`) prints the symbol as text for comparison with other encoders; any first argument other than `L`, `Q`, `H` selects `M`.
* *Unverified:* the claim in the old docs that `zbarimg` decodes all random samples and one maximum payload per version/level was not reproduced here and is not part of the test suite in this repository.
