---
title: The test attribute
parent: quench-test-macros
grand_parent: Crates
nav_order: 1
---

```rust
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream
```

Marks a function as a test of the `quench-test` runner: a replacement for `#[test]` and `#[tokio::test]`. The test binary needs `harness = false` in its `[[test]]` (or `[lib]`) and `quench_test::main!();`.

## Arguments

`#[test]` or `#[test(name = value, ...)]`, comma separated, each a `name = literal` pair:

| Argument | Values | Effect |
|---|---|---|
| `flavor` | `"multi_thread"` or `"current_thread"` | the kind of Tokio runtime for an `async fn` (default `current_thread`). Ignored for a plain `fn`. |
| `worker_threads` | integer literal | number of worker threads of the multi-thread runtime (ignored by the current-thread flavor) |

Anything else is a compile error (below).

## Function shape

* No parameters.
* `fn` or `async fn`. A plain function runs on a test thread; an `async fn` runs on a Tokio runtime built for the test (`enable_all`), via `block_on`.
* Return type `()` or `Result<(), E>` where `E: Debug` (the `Termination` trait of `quench-test`). An `Err` fails the test with `Error: {e:?}`.
* `const fn` and `unsafe fn` are accepted if they compile when called as `name()`.
* Other attributes (`#[doc]`, `#[allow]`, ...) are kept on the function.

## Attributes read by the macro

| Attribute | Effect |
|---|---|
| `#[ignore]` | `ignore: Some("")` |
| `#[ignore = "reason"]` | `ignore: Some("reason")`, printed as `ignored, reason` |
| `#[should_panic]` | `ShouldPanic::Yes` |
| `#[should_panic(expected = "text")]` | `ShouldPanic::WithMessage("text")`: the panic text must contain it |

These two attributes are removed from the function (they only mean something to the standard harness). A `#[should_panic(...)]` list that is not `expected = ".."` is a compile error (see below).

## What it generates

The original function (with `#[allow(dead_code, clippy::future_not_send, clippy::unused_async, clippy::too_many_lines)]` added) followed by

```rust
const _: () = {
    fn __quench_test_run() -> ::quench_test::Outcome {
        ::quench_test::__private::run_sync(|| name())
        // async: ::quench_test::__private::run_async(<multi_thread: bool>, <Option<usize>>, || name())
    }
    ::quench_test::__core::submit!(
        ::quench_test::TESTS,
        ::quench_test::TestCase,
        ::quench_test::TestCase {
            name: concat!(module_path!(), "::", stringify!(name)),
            file: file!(),
            line: line!(),
            ignore: /* None | Some("..") */,
            should_panic: /* ShouldPanic::No | Yes | WithMessage("..") */,
            run: __quench_test_run,
        }
    );
};
```

So the test name is `<module path>::<function>` (the first segment is the crate name: `api::adds` for `tests/api.rs`).

## Compile errors

| Cause | Message |
|---|---|
| `flavor = "weird"` | `` unknown flavor `weird` `` |
| an argument other than `flavor`/`worker_threads`, or a wrong literal type for them | `` unsupported option (use `flavor` or `worker_threads`) `` |
| an argument that is not `name = value` (e.g. `flavor`) | `` expected `name = value` `` |
| a non-literal value (e.g. `flavor = x`) | `expected a literal` |
| the function takes parameters | `a test takes no arguments` |
| applied to something that is not a function | the `syn` parse error for `ItemFn` |
| `#[should_panic(wrong = "x")]` | `` expected `expected = ".."` `` |
