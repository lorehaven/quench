---
title: Testing
parent: quench-log-macros
grand_parent: Crates
nav_order: 2
---

Unit tests in `src/instrument.rs` (std harness) check the expansion text, option parsing and the error messages (for example that `self` is recorded and `skip(self)` removes it). End-to-end behaviour (arguments recorded, `skip`/`skip_all`, `fields`, `level`, `name`, async and sync functions, `self`) is exercised by `quench-log/tests/unit/instrument_tests.rs`.

```bash
cargo test -p quench-log-macros
cargo test -p quench-log --test unit
```

`cargo test --offline -p quench-log-macros`: 9 tests pass.
