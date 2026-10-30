---
title: quench-core
parent: Crates
nav_order: 12
has_children: true
permalink: /crates/quench-core/
---

`quench-core` (library `quench_core`) is the base crate of the workspace: time, UUIDs, text encodings, a catch-all error type, environment and `.env` helpers, a concurrent map, version parsing, byte buffers, directory walking, futures and streams. Its only dependencies are the workspace crate `quench-core-macros` and, on native targets, `getrandom` (part of the allowed set in the [dependency policy](../../guides/dependency-policy.md)).

## Facts

| | |
|---|---|
| Version | 0.1.7 |
| Edition | 2024 (workspace) |
| License | MIT (workspace) |
| Test harness | `harness = false`, `quench_test::main!()` ([testing](testing.md)) |
| Features | none |
| Lints | `[lints] workspace = true` (`unsafe_code = "deny"`, clippy pedantic and nursery denied) |

```toml
[dependencies]
quench-core = { version = "0.1", registry = "ennor" }
```

Inside the workspace use `quench-core = { workspace = true }`.

## What it replaces

The crate-level comment lists: `chrono`, `uuid`, `base32`, `base64`, `anyhow`, `envmnt`, `dashmap`, `semver`, `bytes`, `inventory`, `tempfile`, `dotenvy`, `futures-util`, `walkdir`, `which` and `shlex`. `#[async_trait]` (the `async-trait` crate's role) and `#[derive(Error)]` (`thiserror`'s role) come from [quench-core-macros](../quench-core-macros/index.md).

Everything except `random`, `tmp`, `fs`, `ignore`, `process`, `dotenv` and the `env` setters is pure and builds for `wasm32-unknown-unknown`. On `wasm32` the host installs a clock and an entropy source once ([internals](internals.md)).

## Dependencies

| Dependency | Why | Condition |
|---|---|---|
| `quench-core-macros` | `#[derive(Error)]` and `#[async_trait]`, re-exported from the crate root | always |
| `getrandom` (workspace, `0.4`) | operating-system randomness for `random::try_fill` | `cfg(not(target_arch = "wasm32"))` |
| `quench-test` | dev-dependency, test harness and `#[quench_test::test]` | tests only |

## Modules

| Module | Role | Page |
|---|---|---|
| `error` | catch-all `Error`, `Result`, `Context`, `err!`, `bail!`, `ensure!` | [api](api.md) |
| `env` | environment variable readers and setters | [api](api.md) |
| `base32`, `base64`, `hex` | text encodings | [api](api.md) |
| `uuid` | RFC 4122 UUIDs | [api](api.md) |
| `random` | OS randomness | [api](api.md) |
| `time`, `date` | UTC instants, durations, calendar dates | [api](api.md) |
| `version` | semantic versions and requirements | [api](api.md) |
| `bytes`, `map`, `registry`, `tmp` | buffer, concurrent map, start-up registry, temp dirs | [api-system](api-system.md) |
| `fs`, `ignore`, `dotenv`, `process`, `shell` | walking, ignore rules, `.env`, `which`, shell words | [api-system](api-system.md) |
| `future`, `stream` | `block_on`, `join_all`, `Stream` and adapters | [api-system](api-system.md) |

Root re-exports: `pub use error::{Context, Result};` and `pub use quench_core_macros::{Error, async_trait};`. The root `Error` is the **derive macro** (macro namespace); the catch-all error type is `quench_core::error::Error`.

## Minimal example

```rust
use quench_core::Context;
use quench_core::time::Time;
use quench_core::uuid::Uuid;

fn run() -> quench_core::Result<String> {
    let id = Uuid::new_v4();
    let when = Time::parse_rfc3339("2026-09-29T12:00:00Z").context("bad timestamp")?;
    Ok(format!("{id} at {when}"))
}
```

## Where to go next

* [Usage](usage.md): worked examples.
* [API: errors, encodings, time, versions](api.md) and [API: system and async helpers](api-system.md).
* [Internals](internals.md): the `unsafe` in `env`, the registry constructor, wasm hooks.
* [Testing](testing.md).
* [quench-core-macros](../quench-core-macros/index.md).
