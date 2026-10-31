# quench-log-macros
The `#[instrument]` attribute for `quench-log`.

## What it is
`quench-log-macros` is a proc-macro crate that provides `#[instrument]`. It runs a function inside a span named after it, with its arguments recorded as `Debug` fields. It applies to free functions and methods, sync or `async`.

It is re-exported as `quench_log::instrument`, so use it through `quench-log`. The generated code names `::quench_log::span!`, `::quench_log::Level` and `::quench_log::Instrument`, so the caller must depend on `quench-log` under that name.

## Use cases
- Trace entry into a function with its arguments recorded automatically.
- Keep a span alive across `.await` points in async handlers.
- Leave secrets or non-`Debug` arguments out with `skip` or `skip_all`.
- Attach extra fields and pick the span level or name per function.

## Example
```rust
#[quench_log::instrument]
async fn publish(name: String, version: u32) { /* .. */ }

#[quench_log::instrument(skip(db, body), fields(size = body.len()), level = "debug")]
async fn upload(db: &Db, body: Vec<u8>) { /* .. */ }
```

Options: `skip(a, b)`, `skip_all`, `fields(name = expr)`, `level = "debug"` (default `info`) and `name = "other"`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-log-macros/
