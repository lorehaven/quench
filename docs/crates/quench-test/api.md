---
title: Harness API
parent: quench-test
grand_parent: Crates
nav_order: 2
---

The items of the crate root (`src/lib.rs`, `src/case.rs`, `src/runner.rs`). Modules `cmd`, `mock`, `predicate`, `report` have their own pages. No `unsafe`.

## `macro main!`

```rust
#[macro_export]
macro_rules! main {
    () => {
        fn main() { ::std::process::exit($crate::run_registered()); }
    };
}
```

The entry point of a test binary with `harness = false`: runs every registered test and exits with the harness's status. Use once per test target; for lib unit tests put it under `#[cfg(test)]`.

## `#[test]` (re-export of `quench_test_macros::test`)

`pub use quench_test_macros::test;` See [quench-test-macros](../quench-test-macros/attributes.md) for the arguments (`flavor`, `worker_threads`), the allowed signatures, `#[ignore]`, `#[should_panic]` and the compile errors. Use `use quench_test::test;` or the path `#[quench_test::test]`.

## Registry types (`case`)

### `enum ShouldPanic`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShouldPanic { No, Yes, WithMessage(&'static str) }
```

What a test is expected to do about panicking; `WithMessage` needs the panic text (which starts `thread '..' panicked at ..:`) to contain the string.

### `enum Outcome`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(String),      // it returned an Err
    Panicked(String),    // it panicked: message and location
}
```

### `struct TestCase`

```rust
#[derive(Debug)]
pub struct TestCase {
    pub name: &'static str,            // module path and function name
    pub file: &'static str,
    pub line: u32,
    pub ignore: Option<&'static str>,  // Some(reason) when ignored; the reason may be empty
    pub should_panic: ShouldPanic,
    pub run: fn() -> Outcome,
}
```

A registered test. Normally created by `#[test]`.

### `static TESTS`

```rust
pub static TESTS: quench_core::registry::Registry<TestCase>
```

Every test registered in the binary. Iterate with `TESTS.iter()` (the integration test `dogfood.rs` does).

### `trait Termination`

```rust
pub trait Termination { fn finish(self) -> Result<(), String>; }
impl Termination for ()
impl<E: Debug> Termination for Result<(), E>   // Err(e) fails with "Error: {e:?}"
```

What a test function may return.

### Functions

```rust
pub fn silence_panics(on: bool)
pub fn take_panic_location() -> Option<String>
#[must_use] pub fn panic_message(payload: &(dyn Any + Send)) -> String
```

* `silence_panics(on)`: when `on`, installs the panic hook (once) and makes panics on **every** thread silent (their location is remembered); `false` turns it off again. For runners (such as `quench-test-bdd`) that run work on a pool and report failures themselves. Other threads keep the default hook when silencing is off.
* `take_panic_location()`: `file:line:column` of the last panic on this thread, once (the slot is cleared).
* `panic_message(payload)`: the text of a panic payload (`&str`, `String`, otherwise `"Box<dyn Any>"`).

`#[doc(hidden)]` items used by the macros: `__private` (`run_sync`, `run_async`) and `__core` (`quench_core`).

## Runner (`runner`)

### `struct Options`

```rust
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub filters: Vec<String>,
    pub skips: Vec<String>,
    pub exact: bool,
    pub only_ignored: bool,
    pub include_ignored: bool,
    pub list: bool,
    pub terse: bool,
    pub threads: Option<usize>,
    pub quiet: bool,
    pub report_dir: Option<PathBuf>,
    pub help: bool,
}

impl Options {
    #[must_use] pub fn parse(args: &[String]) -> Self
}
```

`parse` reads the arguments **after** the program name; unknown flags are ignored. The meaning of each field is the [command-line table](configuration.md).

### `struct RunResult`

```rust
#[derive(Debug)]
pub struct RunResult {
    pub exit_code: i32,       // 0 when nothing failed, 101 otherwise
    pub report: report::Report,
}
```

### `fn run`

```rust
pub fn run(
    cases: &[&'static TestCase],
    options: &Options,
    out: &mut (dyn Write + Send),
) -> RunResult
```

Runs the selected cases and prints in the standard harness's format to `out`; does not write report files. Selected tests are sorted by name. Ignored tests are listed first (`test NAME ... ignored[, reason]`). Tests run on `threads` worker threads taking the next test from a shared counter. Each test is run under `catch_unwind` with panics captured: a panic's message and location go to the failure report instead of stderr. A test counts as passed when: it passed and `should_panic` is `No`; or it panicked and `should_panic` is `Yes`; or it panicked with the expected text. A test that returned `Err` fails even when a panic was expected. Failure messages: `test did not panic as expected`; `panic did not contain expected string` with the panic message and the expected substring. With `list`, prints names only (`name: test` and a count line unless `terse`) and returns exit code 0.

### `fn run_registered`

```rust
#[must_use]
pub fn run_registered() -> i32
```

What `main!` calls: reads `std::env::args()`, prints help for `-h/--help` (returns 0), otherwise `run(&TESTS..., &options, &mut stdout)`, writes the reports (unless `--list`, `QUENCH_TEST_REPORT=off`, or nextest; a write error is a warning on stderr: `warning: could not write the test report to <dir>: <error>`) and returns the exit code.

## Re-exports

`Outcome`, `ShouldPanic`, `TESTS`, `Termination`, `TestCase`, `panic_message`, `silence_panics`, `take_panic_location` from `case`; `Options`, `RunResult`, `run`, `run_registered` from `runner`; modules `cmd`, `mock`, `predicate`, `report`.
