---
title: quench-log
parent: Crates
nav_order: 29
has_children: true
permalink: /crates/quench-log/
---

`quench-log` (library `quench_log`) is the logging used by every service and CLI of the estate: leveled events with fields, spans, plain-text console output, and a rolling file (JSON by default), both at once, configured from a handful of environment variables. It replaces `tracing`, `tracing-subscriber` and `tracing-appender`.

| | |
|---|---|
| Version | 0.1.9 |
| Edition | 2024 (workspace) |
| Features | none |
| Test harness | `harness = false`, `tests/unit.rs` with `quench_test::main!()` |

```toml
[dependencies]
quench-log = { version = "0.1", registry = "ennor" }
```

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `Time`, the `Error` derive, `env::var` |
| `quench-serde` | JSON lines (`Map`, `Value`, `json::to_string`) |
| `quench-log-macros` | the `#[instrument]` attribute, re-exported as `quench_log::instrument` |
| `quench-test` (dev) | harness |

`quench-starter::logging::init()` delegates to this crate; a program that wants nothing else of `quench-starter` can depend on `quench-log` alone.

## Minimal example

```rust
fn main() {
    quench_log::init(); // reads RUST_LOG and the LOG_* variables, see Config::from_env
    let name = "ada";
    quench_log::info!(port = 8080, user = %name, "ready");
}
```

Console line: `2026-10-23T12:00:00.123456Z  INFO my_crate: ready port=8080 user=ada`.

## Where to go next

* [Usage](usage.md): events, spans, `#[instrument]`, tests.
* [API](api.md): every public item.
* [Configuration](configuration.md): `Config`, environment variables, filter syntax.
* [Internals](internals.md), [Testing](testing.md), [quench-log-macros](../quench-log-macros/index.md).
