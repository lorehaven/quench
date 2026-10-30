---
title: Testing
parent: quench-log
grand_parent: Crates
nav_order: 5
---

* Harness: `harness = false` for the library and `tests/unit.rs`, which mounts four modules via `#[path]`: `config_tests`, `init_tests`, `instrument_tests`, `macros_tests`, and ends with `quench_test::main!()`.
* Unit tests sit in the sources (`#[quench_test::test]` counts: `field` 3, `appender` 7, `format` 6, `dispatch` 7, `level` 8, `lib` 1).
* `tests/unit/` covers: all level macros and field forms, disabled-level laziness, targets, spans and `record`, instrumented futures, `Config` parsing and `from_env`, the real file output (JSON and plain, file naming, directory creation, sink toggles), error messages, and `#[instrument]`.
* The global dispatcher can be installed once per process, so every test except one builds a dispatcher with `subscriber()` and scopes it with `with_default`; the exception checks that a second `try_init_with` fails.

## Testing code that logs

```rust
use quench_log::{Dispatcher, Filter, Level, MemorySink};

let sink = MemorySink::new();
let dispatcher = Dispatcher::new(Filter::level(Level::Debug)).with_sink(sink.clone());
quench_log::with_default(dispatcher, || {
    quench_log::info!(n = 1, "hello");
});
assert_eq!(sink.records()[0].message, "hello");
```

`with_default` is per thread: events from other threads do not reach it.

```bash
cargo test -p quench-log
```

## Result of the last run

`cargo test --offline -p quench-log`: 31 unit tests and 54 in `tests/unit` pass; the crate-level `no_run` doctest compiles and the `event!` doc example is `ignore`d.
