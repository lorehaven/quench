---
title: quench-test
parent: Crates
has_children: true
permalink: /crates/quench-test/
nav_order: 48
---

`quench-test` (library `quench_test`) is the test harness of the workspace. A test target sets `harness = false` and calls `quench_test::main!()`; `#[quench_test::test]` replaces `#[test]` and `#[tokio::test]`. The binary accepts the command line `cargo test` passes, prints what the standard harness prints, runs tests on a thread pool, and writes JSON and HTML result pages. The crate also carries what service tests need around it: a command runner (`assert_cmd`), predicates (`predicates`) and a mock HTTP server (`wiremock`).

## Facts

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 |
| License | MIT |
| Features | none |
| Test harness | `autotests = false`; four integration tests (`cmd`, `dogfood`, `mock`, `report`), all `harness = false` |

```toml
[dev-dependencies]
quench-test = { version = "0.1.3", registry = "ennor" }

[[test]]
name = "api"
path = "tests/api.rs"
harness = false
```

The `[[test]]` entry is required for every test target that uses the harness; with `autotests = false` (as in this workspace) every integration test target needs one. For unit tests inside a library, set `harness = false` in `[lib]` and end `src/lib.rs` with `#[cfg(test)] quench_test::main!();`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | the test registry (`registry::Registry`, `submit!`), `env`, `base64` |
| `quench-regex` | `predicate::str::is_match`, `matchers::path_regex`, `header_regex` |
| `quench-serde` | the JSON report, `body_json`, `Value` |
| `quench-test-macros` | the `#[test]` attribute (see [quench-test-macros](../quench-test-macros/index.md)) |
| `tokio` | async tests, the mock server |

## Where to go next

* [Usage](usage.md)
* [Harness API](api.md): `main!`, `#[test]`, `TestCase`, `Outcome`, `Options`, `run`.
* [Command runner and predicates](api-cmd.md)
* [Mock HTTP server](api-mock.md)
* [Reports](api-report.md): the result model, JSON/HTML files and where they go.
* [Configuration](configuration.md): command-line options and environment variables.
* [Internals](internals.md)
* [Testing](testing.md)
* Gherkin tests: [quench-test-bdd](../quench-test-bdd/index.md). The guide: [Testing with quench-test and quench-test-bdd](../../guides/testing.md).

## Minimal example

```rust,ignore
// tests/unit.rs, with `[[test]] name = "unit" harness = false` in Cargo.toml
use quench_test::test;

#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn fetches() -> Result<(), String> {
    Ok(())
}

quench_test::main!();
```
