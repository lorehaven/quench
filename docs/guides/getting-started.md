---
title: Getting started
parent: Guides
nav_order: 1
---

How to get the workspace, what is in it, and how to use your first crate. Everything here was checked against the repository's manifests and by building a small program against the crates.

## Prerequisites

* A Rust toolchain that supports **edition 2024** (set once in `[workspace.package]` in the root `Cargo.toml`; every crate inherits it with `edition = { workspace = true }`). No `rust-version` is declared anywhere, so there is no stated MSRV beyond that.
* `cargo test`/`cargo build` need nothing else. The helper scripts in `scripts/` are written for `fish`; the security gate additionally needs `cargo-deny`, `cargo-audit` and, for fuzzing, `cargo-fuzz` with a nightly toolchain (see [Dependency policy](dependency-policy.md)).

## Workspace layout

The root `Cargo.toml` is a Cargo workspace (`resolver = "2"`) with 61 member crates, all named `quench-*`, plus `examples/*`.

| Path | Content |
|---|---|
| `quench-<name>/` | one crate each; library name `quench_<name>`; `-macros` directories are proc-macro crates |
| `examples/` | `basic`, `db_example`, `forge_service_demo`, `http_demo`, `native_gallery`, `native_window`, `ui_web_app`, `ui_web_server` (each a workspace member; their package names start with `quench-example-`, except `quench-db-example`) |
| `docs/` | this book |
| `scripts/` | `security.fish`, `deps.fish`, `features.fish`, `coverage.fish`, `test-env.fish`, `prune-deny.py`, `unsafe-allowlist.txt` and the Android/Linux end-to-end helpers |
| `fuzz/` | the libFuzzer targets (a separate workspace) |
| `deny.toml`, `.cargo/audit.toml` | supply-chain policy |
| `.conveyor.toml`, `.anvil.toml`, `foreman.toml` | CI pipeline, release configuration, local service orchestration |

`[workspace.package]` carries edition, authors, homepage/repository (`https://github.com/lorehaven/quench`), license (MIT) and readme. `[workspace.dependencies]` lists every workspace crate as `{ path = "...", registry = "ennor", version = "..." }` plus the allowed third-party crates ([Dependency policy](dependency-policy.md)); member crates refer to them with `{ workspace = true }`. Release profile settings: `strip = "debuginfo"`; a `wasm` profile (`opt-level = "z"`, `lto`, `panic = "abort"`) for `cargo build --target wasm32-unknown-unknown --profile wasm -p <app>`.

## Build and test

```bash
cargo build --workspace
cargo test -p quench-core             # one crate
cargo test --workspace                # everything that needs no external service
```

Tests use `quench-test`'s own harness (`harness = false`); see [Conventions](conventions.md). The database and cache tests need Postgres and Redis (`scripts/test-env.fish` exports `DB_TEST_POSTGRES_URL` and `CACHE_TEST_REDIS_URL` for the containers in `foreman.toml`).

## Use a crate in your project

Inside the workspace, depend on a crate with `quench-core = { workspace = true }`. From outside, the crates are published to the registry named `ennor` (the name is used in `.anvil.toml` `[release] registry = "ennor"` and in `[workspace.dependencies]`; the registry's URL is configured outside the repository):

```toml
[dependencies]
quench-core = { version = "0.1", registry = "ennor" }
quench-serde = { version = "0.1", registry = "ennor" }
quench-log = { version = "0.1", registry = "ennor" }
```

Or, from a checkout, by path: `quench-core = { path = "/path/to/quench/quench-core" }`.

## A first program

This uses `quench-core` (errors, time), `quench-serde` (derive and JSON) and `quench-log`:

```rust
use quench_core::Context;
use quench_core::time::{Duration, Time};
use quench_serde::{Deserialize, Serialize, json};

#[derive(Debug, Serialize, Deserialize)]
struct User {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    note: Option<String>,
}

fn run() -> quench_core::Result<()> {
    let user: User = json::from_str(r#"{"name":"ada"}"#)?;
    quench_log::info!(user = ?user, "loaded");

    let at = Time::parse_rfc3339("2026-09-29T12:00:00+02:00").context("bad timestamp")?;
    let later = at + Duration::minutes(90);
    quench_log::info!(when = %later, "scheduled"); // 2026-09-29T11:30:00Z
    println!("{}", json::to_string(&user));       // {"name":"ada","tags":[],"note":null}
    Ok(())
}

fn main() -> quench_core::Result<()> {
    quench_log::init_console();
    run()
}
```

Output (the log timestamp varies):

```text
2026-10-30T15:35:26.031299Z  INFO first: loaded user=User { name: "ada", tags: [], note: None }
2026-10-30T15:35:26.031318Z  INFO first: scheduled when=2026-09-29T11:30:00Z
{"name":"ada","tags":[],"note":null}
```

The program was built and run as a scratch crate with path dependencies on these crates; the log target (`first`) is the scratch crate's name.

## Next

* [Architecture](architecture.md): which crates depend on which.
* [Conventions](conventions.md) and [Dependency policy](dependency-policy.md).
* The per-crate reference: [Crates](../crates/index.md), starting with [quench-core](../crates/quench-core/index.md), [quench-serde](../crates/quench-serde/index.md) and [quench-log](../crates/quench-log/index.md).
