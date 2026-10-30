---
title: Testing
parent: quench-serde-macros
grand_parent: Crates
nav_order: 2
---

`src/tests.rs` (compiled under `cfg(test)`, std test harness) runs the expansion functions on 14 sample shapes for both derives and asserts on the token text: output shapes for `Serialize` and `Deserialize`, generic bounds, rename rules, enum representations, and the refusals (unions, `flatten` on a tuple field, internally tagged tuple variants, `tag` with `untagged`, unsupported container/field/variant attributes, non-Rust input). End-to-end behaviour is covered by `quench-serde/tests/derive.rs`.

```bash
cargo test -p quench-serde-macros
cargo test -p quench-serde --test derive
```

`cargo test --offline -p quench-serde-macros`: 15 tests pass.
