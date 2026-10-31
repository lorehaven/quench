# quench-test-macros
The `#[test]` attribute of `quench-test`.

## What it is
`quench-test-macros` is the proc-macro crate behind `quench-test`. It provides the single attribute `#[test]`, re-exported as `quench_test::test`, a replacement for `#[test]` and `#[tokio::test]`. The generated code names `::quench_test::..`, so the crate using it must depend on `quench-test` as a dev-dependency.

The test binary needs `harness = false` and `quench_test::main!();`. The function takes no parameters, may be `async`, and returns `()` or `Result<(), E>` with `E: Debug`.

## Use cases
- Write sync tests that register with the `quench-test` runner.
- Write async tests on a current-thread or multi-thread Tokio runtime.
- Mark tests with `#[ignore]` or `#[should_panic(expected = "...")]`.
- Return `Result` from tests so an `Err` fails with its debug text.

## Example
```rust
#[quench_test::test]
async fn current_thread() {
    tokio::task::yield_now().await;
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi() -> Result<(), std::io::Error> {
    Ok(())
}

#[quench_test::test]
#[should_panic(expected = "needle")]
fn panics_with_text() {
    panic!("a needle in a haystack");
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-test-macros/
