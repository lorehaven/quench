---
title: Testing
parent: quench-security
grand_parent: Crates
nav_order: 5
---

`tests/scanners.rs` (always on, `harness = false`, `quench_test::main!()`) has eight cases: unsafe in every spelling is noticed and comments are not; secrets are caught by shape; the filesystem scan finds a planted violation and skips build output; allowlists ignore comments and blanks; fuzz statistics are read from the final line; a missing program is a failed run with a reason; inside a git checkout only project files are scanned; symlinks that leave the project are found and are not followed by the secret scan.

`tests/gate.rs` (9 cases: 2 supply chain, 3 static, 4 attack suites; see [index](index.md)) and `tests/fuzz.rs` (8 cases) need `--features gate`.

```bash
cargo test -p quench-security --test scanners
```

`cargo test --offline -p quench-security --test scanners`: 8 tests pass. The `gate` and `fuzz` tests were not run for this documentation (they need the network and extra tools).
