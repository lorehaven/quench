---
title: Usage
parent: quench-test
grand_parent: Crates
nav_order: 1
---

## 1. Set a test target up

In `Cargo.toml` (every test target is listed because the workspace uses `autotests = false`):

```toml
[lib]
harness = false          # only if the library has unit tests

[dev-dependencies]
quench-test = { workspace = true }

[[test]]
name = "api"
path = "tests/api.rs"
harness = false
```

In `tests/api.rs`:

```rust,ignore
use quench_test::test;

#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}

quench_test::main!();
```

`cargo test`, `cargo test adds`, `cargo test -- --exact api::adds` and `cargo test -- --list` work as with the standard harness.

For unit tests inside a library, `src/lib.rs` ends with:

```rust,ignore
#[cfg(test)]
mod tests;

#[cfg(test)]
quench_test::main!();
```

(`src/tests.rs` uses `#[quench_test::test]`.) Pitfall: a test target without `harness = false` runs the standard harness, where `#[quench_test::test]` functions are never executed (they register into `TESTS`, which only `main!` reads).

## 2. Async tests and `Result`

```rust,ignore
#[quench_test::test]
async fn current_thread() {
    tokio::task::yield_now().await;
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi() -> Result<(), std::io::Error> {
    tokio::spawn(async { 1 + 1 }).await.unwrap();
    Ok(())
}
```

Each async test gets its own Tokio runtime (current-thread by default). A test returning `Err(e)` fails with `Error: {e:?}`.

## 3. Expected panics and ignored tests

```rust,ignore
#[quench_test::test]
#[should_panic(expected = "needle")]
fn panics_with_text() {
    panic!("a needle in a haystack");
}

#[quench_test::test]
#[ignore = "needs the network"]
fn online() {}
```

`cargo test -- --ignored` runs only ignored tests, `--include-ignored` runs everything.

## 4. Run a binary and check how it ended

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
        .stdout(predicate::str::contains("Usage:").and(predicate::str::is_empty().not()));
}
```

Standard input is `/dev/null` unless you call `write_stdin`; a tool that prompts therefore sees end-of-input. For tools that read a terminal (such as `quench-cli`'s REPL) this is what you want in tests. Pitfall: `stdout("text")` is an **exact** comparison of the whole output; use `predicate::str::contains` for a part.

## 5. Mock the other side of an HTTP call

```rust,ignore
use quench_test::mock::{Mock, MockServer, ResponseTemplate, matchers::{header, method, path}};

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn calls_the_api() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer t"))
        .respond_with(ResponseTemplate::new(200).set_body_string("[]").insert_header("x-a", "b"))
        .expect(1)
        .mount(&server)
        .await;
    let base = server.uri();   // hand this to the code under test
    // ... call `{base}/v1/models` ...
    server.verify().await;
}
```

Use `up_to_n_times(1)` plus a second mock to script "fail once, then succeed", and `with_priority` to put a catch-all behind specific rules. `server.received_requests().await` lists what arrived, for assertions on bodies.

## 6. Read the reports

`cargo test` leaves `target/debug/quench-test/index.html` (all test binaries, filterable), `summary.json`, and one `<package>-<binary>.json/.html` per binary. Set `QUENCH_TEST_REPORT=off` to write nothing, or `QUENCH_TEST_REPORT_DIR=out` / `--report-dir out` to write elsewhere. Under `cargo nextest` nothing is written unless `QUENCH_TEST_REPORT_DIR` is set.

## 7. Keep output tidy

`-q` prints one character per test, `--test-threads 1` serialises tests (use it for tests that share a process-wide resource such as an environment variable). Tests run in parallel by default on one thread per available core.
