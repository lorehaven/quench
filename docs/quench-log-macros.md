# Quench Log Macros

`quench-log-macros` (crate `quench_log_macros`) holds the `#[instrument]` attribute for [Quench Log](./quench-log.md). Use it through that crate: `quench_log::instrument` is a re-export. It is a proc-macro crate built on `syn`/`quote`, like the other quench macro crates ([Quench Core Macros](./quench-core-macros.md), [Quench Serde Macros](./quench-serde-macros.md), `quench-http-macros`).

## `#[instrument]`

Runs a function inside a span named after it, with its arguments recorded as `Debug` fields:

```rust
#[quench_log::instrument]
async fn publish(name: String, version: u32) { /* .. */ }

#[quench_log::instrument(skip(db, body), fields(size = body.len()), level = "debug")]
async fn upload(db: &Db, body: Vec<u8>) { /* .. */ }
```

| Option | Meaning |
|---|---|
| `skip(a, b)` | leave these arguments out (do this for anything that is not `Debug`, or is a secret) |
| `skip_all` | leave out every argument |
| `fields(name = expr, other = %expr)` | extra fields, with the same syntax as the `span!` macro (`%` for `Display`, `?` for `Debug`) |
| `level = "debug"` | the span's level (default `info`) |
| `name = "other"` | a different span name (default: the function's name) |

An `async fn` is instrumented as a future, so the span is entered on every poll and the events logged inside it keep their fields across `.await`s and task migration; a plain `fn` holds the span's guard for the duration of the call. `self` is never recorded unless you add it through `fields`.

## Testing

The attribute parser and expansion have unit tests in the crate (including the error messages for unsupported options); the generated code is exercised end to end by `quench-log/tests/unit/instrument_tests.rs` (arguments recorded, `skip`/`skip_all`, `fields`, `level`, `name`, async and sync functions).

[Home](../README.md)
