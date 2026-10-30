---
title: Testing with quench-test and quench-test-bdd
parent: Guides
nav_order: 9
---

How tests are written and laid out in this workspace: the `quench-test` harness (`main!()`, `#[quench_test::test]`, async tests, the command runner and the mock server), Gherkin tests with `quench-test-bdd`, and the conventions every crate follows. Reference: [quench-test](../crates/quench-test/index.md), [quench-test-bdd](../crates/quench-test-bdd/index.md).

## Why a custom harness

Cargo's standard harness is replaced by `quench-test`'s runner (`harness = false`). The binary still understands the arguments `cargo test` passes and prints the same lines (`running N tests`, `test x ... ok`, `test result: ok. ...`, exit code 101 on failure), so `cargo test`, `cargo test name -- --exact`, `--list`, `--ignored`, `--test-threads` work unchanged. In addition every run writes JSON and HTML result pages (`target/debug/quench-test/index.html`), and `async` tests, `Result` returns, a command runner, predicates and a mock HTTP server come with it, so no `tokio::test`, `assert_cmd`, `predicates` or `wiremock` is needed.

## The shape of a test target

A test binary is a Rust file with `#[quench_test::test]` functions and one `quench_test::main!();`:

```rust,ignore
use quench_test::test;

#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn fetches() -> Result<(), String> {
    Ok(())
}

#[test]
#[should_panic(expected = "needle")]
fn panics_with_text() {
    panic!("a needle in a haystack");
}

#[test]
#[ignore = "needs the network"]
fn online() {}

quench_test::main!();
```

* Plain `fn` tests run on a pool thread; `async fn` tests get their own Tokio runtime (`flavor = "multi_thread"` and `worker_threads = N` select a multi-thread one).
* A test may return `()` or `Result<(), E: Debug>`; an `Err` fails it.
* `#[ignore]`, `#[ignore = "why"]`, `#[should_panic]`, `#[should_panic(expected = "text")]` behave as in the standard harness.
* Tests run in parallel (one thread per core by default). Anything process-wide (environment variables, the current directory, fixed ports) needs a lock or `--test-threads=1`.

The macro only registers the function; **the registered tests run only if the binary uses `main!()`**. A target left on the standard harness silently runs nothing.

## How the workspace is laid out

Every crate of the workspace except the procedural-macro crates and `fuzz` sets `harness = false` (52 of 62 manifests). The pieces:

### Cargo.toml

```toml
[package]
autotests = false          # so every test target is listed explicitly

[lib]
harness = false            # the library's own unit tests use quench-test too

[dev-dependencies]
quench-test = { workspace = true }

[[test]]
name = "unit"
path = "tests/unit.rs"
harness = false
```

### Unit tests of a library

`src/lib.rs` ends with the `tests` module and the entry point, both under `cfg(test)`. This is the pattern of `quench-args`, `quench-regex` and `quench-schema`:

```rust,ignore
#[cfg(test)]
mod tests;

#[cfg(test)]
quench_test::main!();
```

The tests live in `src/tests.rs` (or inline `#[cfg(test)] mod tests { ... }` in a module, as in `quench-cli`'s `style.rs`), each marked `#[quench_test::test]`.

### Integration tests

Each file `tests/<name>.rs` is its own binary with its own `[[test]]` entry and its own `quench_test::main!();` (for example `quench-http/tests/e2e.rs`). A crate with many small unit tests keeps them as modules of one binary instead, `tests/unit.rs`, which only lists the files:

```rust,ignore
// tests/unit.rs
#![allow(clippy::wildcard_imports)]

#[path = "unit/common_db_tests.rs"]
mod common_db_tests;
#[path = "unit/metrics_tests.rs"]
mod metrics_tests;

quench_test::main!();
```

(`quench-starter` and `quench-cli` use this shape.) The inner `#![allow(clippy::wildcard_imports)]` is there because a custom harness does not set `cfg(test)`, so Clippy treats these files as ordinary code.

### Proc-macro crates

Crates with `proc-macro = true` (`quench-args-macros`, `quench-test-macros`, `quench-test-bdd-macros`, ...) keep the standard harness: they test their expansion functions with plain `#[test]` and `syn::parse_quote!`, and the behaviour of the generated code is tested in the crate that re-exports the macros.

### Running

```bash
cargo test --workspace                          # everything
cargo test -p quench-regex                      # one crate
cargo test -p quench-regex -- is_match --exact  # name filters as usual
cargo test -p quench-cli </dev/null             # close stdin for tests that read a terminal
```

The reports of the run are in `target/debug/quench-test/` (`index.html` merges all test binaries; set `QUENCH_TEST_REPORT=off` to write nothing). The Redis-backed cache tests are skipped unless `CACHE_TEST_REDIS_URL` is set (see the repository README); `scripts/test-env.fish` sets `DB_TEST_POSTGRES_URL` and `CACHE_TEST_REDIS_URL` for the containers of `foreman.toml`.

## Testing a binary

`quench_test::cmd::Command` runs a program and asserts on its exit status and output; `Command::cargo_bin("name")` finds a binary target of the package under test (`CARGO_BIN_EXE_<name>`, set by Cargo for integration tests).

```rust,ignore
use quench_test::cmd::Command;
use quench_test::predicate::prelude::*;

#[quench_test::test]
fn help_works() {
    Command::cargo_bin("mytool")
        .unwrap()
        .arg("--help")
        .env("NO_COLOR", "1")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}
```

`stdout("text")` is an exact comparison of the whole output; `predicate::str::contains`, `starts_with`, `ends_with`, `is_empty`, `is_match(regex)` and `.and(..)`, `.or(..)`, `.not()` build partial checks. Standard input is `/dev/null` unless `write_stdin(bytes)` is called. See [Building command-line tools](building-cli-tools.md) for a complete tool and its tests.

## Testing HTTP clients

```rust,ignore
use quench_test::mock::{Mock, MockServer, ResponseTemplate, matchers::{header, method, path}};

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn calls_the_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer t"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
        .expect(1)
        .mount(&server)
        .await;
    // ... point the client at server.uri() and call it ...
    server.verify().await;      // also checked when `server` is dropped
}
```

The server listens on `127.0.0.1` on a free port in its own thread. Requests that match no rule get an empty 404 and are recorded (`server.received_requests().await`). Use `up_to_n_times(n)` and `with_priority(p)` to script sequences.

## Gherkin tests with quench-test-bdd

Feature files describe behaviour in plain language; Rust functions implement the steps.

`tests/features/calc.feature`:

```text
@math
Feature: Calculator
  Background:
    Given a total of 1

  Scenario: Adding
    When I add 2
    Then the total is 3

  Scenario Outline: Adding <n>
    When I add <n>
    Then the total is <result>

    Examples:
      | n | result |
      | 1 | 2      |
      | 5 | 6      |
```

`tests/bdd.rs`, listed as a `harness = false` test target so it has its own `main`:

```rust,ignore
use quench_test_bdd::{World, given, then, when};

#[derive(World, Default)]
struct Calc {
    total: i64,
}

#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) {
    world.total = n;
}

#[when("I add {int}")]
fn add(world: &mut Calc, n: i64) {
    world.total += n;
}

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) -> Result<(), String> {
    if world.total == n {
        Ok(())
    } else {
        Err(format!("total is {}", world.total))
    }
}

#[tokio::main]
async fn main() {
    let summary = quench_test_bdd::Runner::<Calc>::new()
        .title("calc")
        .run("tests/features")
        .await;
    std::process::exit(i32::from(summary.failed()));
}
```

* Step functions take `&mut World` first and the captures after it, parsed with `FromStr`; they may be `async` and may return `Result<(), E: Display>`. A panic also fails the step.
* Cucumber expressions: `{int}`, `{float}`, `{word}`, `{string}` (quoted), `{}`, `cucumber(s)`, `is/are`. Regex steps: `#[given(regex = r"^...$")]`. See [syntax](../crates/quench-test-bdd/syntax.md).
* Each scenario gets a fresh world (`#[derive(World)]` uses `Default`, or `#[world(init = path)]` with an `async fn() -> Self`).
* Scenarios run concurrently (64 at a time by default). Tag a scenario `@serial` to run it alone, after the others.
* A failed step skips the rest of its scenario; undefined and ambiguous steps fail the scenario.
* Select what runs with `Options` (tag expression `@fast and not @slow`, name regex, `fail_fast`, `concurrency`, `report_dir`, `quiet`); flatten it into your own command line (see [configuration](../crates/quench-test-bdd/configuration.md)) or build it in code.
* The reports (`calc.json`, `calc.html`) go to the same directory as the unit-test reports and are merged into the same `index.html`, with the steps of each scenario.

The console output of a run (`scenario Adding ... ok`, a `failures:` section naming the feature, scenario, line and failed step, and a final `features: ...; scenarios: ...; steps: ...` line) is shown with real output on the [quench-test-bdd](../crates/quench-test-bdd/index.md) page.

## Pitfalls

* No `harness = false` or no `main!()`: tests silently do not run.
* `autotests = false` means a new `tests/foo.rs` is ignored until it has a `[[test]]` entry.
* Environment variables and other global state are shared by parallel tests; serialise with a `static Mutex` or run with `--test-threads=1`.
* The output of `println!` in a test is not captured; it appears immediately, interleaved across threads.
* `Command` starts programs with closed standard input; an interactive tool needs `write_stdin` (and a non-terminal stdin makes `quench-cli`'s editor read plain lines).
* A mock server verifies its `expect` counts when dropped and **panics** then, unless the thread is already panicking.
