---
title: Conventions
parent: Guides
nav_order: 4
---

The conventions visible across the workspace, with where each is encoded.

## Manifests

* Every crate manifest inherits `edition`, `authors`, `homepage`, `repository`, `license` and `readme` from `[workspace.package]` (`edition = "2024"`, license MIT).
* Dependencies on other workspace crates and on third-party crates use `{ workspace = true }`; versions and the `ennor` registry live only in the root `[workspace.dependencies]`.
* Directory `quench-<name>` has library name `quench_<name>`; macro crates are `quench-<name>-macros` with `[lib] proc-macro = true` (nine of them) and are re-exported by their parent crate (`pub use quench_serde_macros::{Deserialize, Serialize}`).
* Every library crate sets `[lints] workspace = true`.
* Crate-level `//!` docs say what the crate replaces ("`anyhow`'s role", "`thiserror`'s role").

## Lints

From the root manifest:

```toml
[workspace.lints.rust]
unsafe_code = "deny"
missing_debug_implementations = "deny"

[workspace.lints.clippy]
all, cargo, complexity, correctness, nursery, pedantic, perf, style, suspicious = deny (priority -1)
missing_errors_doc = "allow"
missing_panics_doc = "allow"
multiple_crate_versions = "allow"
fn_params_excessive_bools = "allow"
```

Consequences seen in the code: every public type derives or implements `Debug`; `#[must_use]` on pure functions (clippy pedantic); `# Errors` doc sections where the author chose to write them; exceptions are local `#[allow(...)]` with a reason. CI runs `anvil lint --all-targets --deny-warnings` and `anvil format --check` (rustfmt default settings: there is no `rustfmt.toml`). New `unsafe` needs an entry in `scripts/unsafe-allowlist.txt` ([Dependency policy](dependency-policy.md)).

## Errors

* Library crates define their own error enums with `#[derive(Error)]` from `quench_core` (for example `quench_log::Error`, `quench_config::ConfigError`) or a small struct with `Display` and `std::error::Error` (`quench_core::base64::DecodeError`, `quench_tz::UnknownZone`).
* Application and glue code uses `quench_core::Result` and the `Context` trait (`.context("..")`), and prints the chain with `{:#}` or by returning `Err` from `main`.
* Message styles differ between crates: `quench-log` and `quench-core` use lower-case sentences (`could not create log directory ...`), `quench-config` capitalises (`Failed to read config file: ...`). There is no single rule in the code.
* Parse errors carry a position where there is one (`quench_serde::Error::location`, `path`).

## Testing style

* Tests run on `quench-test`'s own harness, not libtest: libraries and integration tests declare `harness = false` (52 crates do), `autotests = false`, explicit `[[test]]` entries, and end the file with `quench_test::main!();`. Tests are marked `#[quench_test::test]`.
* Unit tests live next to the code in `#[cfg(test)] mod tests`; larger crates use `tests/unit.rs` that mounts `tests/unit/<name>_tests.rs` with `#[path]`.
* Because the custom harness does not set `cfg(test)`, integration test files start with `#![allow(clippy::wildcard_imports)]`.
* Proc-macro crates keep the expansion in plain functions and unit-test them with the standard `#[test]` and a string of tokens; behaviour is tested from a consumer crate (`quench-core/tests/error_derive.rs`).
* Tests that change the environment use variable names unique to the test (`quench_core::env::set` is only safe that way).
* Reports: `QUENCH_TEST_REPORT_DIR` receives `summary.json` and `index.html`; CI keeps them.
* `scripts/coverage.fish` gates per-crate line coverage (`MIN_COVERAGE`, default 90 percent; `NO_FAIL=1` prints only); `scripts/features.fish` tests the feature matrix.
* Tests that need Postgres or Redis read `DB_TEST_POSTGRES_URL` and `CACHE_TEST_REDIS_URL` (`scripts/test-env.fish`).

## API style

* Builders and constructors that return `Self` are `#[must_use]`; closures instead of guards for shared state (`ShardedMap`); `const fn` where possible.
* Types that cross crate boundaries are plain data: `Value`/`Map` for formats, `Time`/`Duration`/`Date` for time, `Bytes` for bodies.
* Pure modules build for `wasm32`; platform hooks are installed once by the host (`time::set_clock`, `random::set_source`).

## Releases

Version bumps are made per crate; the log shows commits of the form `release: bump package versions (quench-raster v0.1.3, ...)`. `.anvil.toml` lists the packages to release to the registry `ennor` and installs `quench-tool` locally.
