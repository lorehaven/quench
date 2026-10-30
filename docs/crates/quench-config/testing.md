---
title: Testing
parent: quench-config
grand_parent: Crates
nav_order: 5
---

`tests/unit.rs` (`harness = false`, `quench_test::main!()`) mounts `tests/unit/lib_tests.rs`: prefixed and bare environment lookup, defaults, `env_u64` parsing, every truthy spelling for `env_bool`, an unrecognised present value being false, `env_list` trimming, JSON/YAML/TOML round trips and error paths, and `load_with_fallback` (file by extension, environment variable when no path is given or the path does not exist). The tests set variables with `quench_core::env::set` under unique names and write files under the OS temp directory with the process id in the name.

```bash
cargo test -p quench-config
```

`cargo test --offline -p quench-config`: 22 tests pass.
