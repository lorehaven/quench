# quench-test
A light test runtime with JSON and HTML result reports, plus a command runner, predicates and a mock HTTP server for tests.

## What it is
`quench-test` is the test harness of the workspace. A test target sets `harness = false` and calls `quench_test::main!()`; `#[quench_test::test]` replaces `#[test]` and `#[tokio::test]`. The binary accepts the command line `cargo test` passes, prints what the standard harness prints, runs tests on a thread pool, and writes JSON and HTML result pages.

The crate also carries what service tests need around it: a command runner (`assert_cmd`), predicates (`predicates`) and a mock HTTP server (`wiremock`). The `#[test]` attribute comes from `quench-test-macros`.

## Use cases
- Run unit and integration tests with sync or async functions.
- Get JSON and HTML test reports.
- Run a binary and assert on its exit status and output.
- Match output with predicates.
- Mock an upstream HTTP server in service tests.

## Example
```toml
[dev-dependencies]
quench-test = { workspace = true }

[[test]]
name = "api"
path = "tests/api.rs"
harness = false
```

```rust
use quench_test::test;

#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}

#[test(flavor = "multi_thread", worker_threads = 2)]
async fn multi() -> Result<(), std::io::Error> {
    tokio::spawn(async { 1 + 1 }).await.unwrap();
    Ok(())
}

quench_test::main!();
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-test/
