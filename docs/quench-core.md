# Quench Core

`quench-core` (crate `quench_core`) is the dependency-free base every other quench crate builds on. It replaces `chrono`, `uuid`, `base64`, `anyhow`, `envmnt`, `dashmap`, `semver`, `bytes`, `tempfile`, `futures-util`, `async-trait` and `inventory`. Its only dependency is `getrandom`, and only on native targets: the pure modules build for `wasm32-unknown-unknown`, which is what lets shared types compile on both the server and the browser side.

## Modules

- **`time`** — `Time` (a UTC instant, nanosecond resolution) and `Duration` (signed). `Time::now()`, `from_unix*`, `from_ymd_hms`, `civil()` (calendar fields), `weekday()`, `+`/`-` with `Duration`, `Time - Time`. Text: `to_rfc3339()` (also `Display`; fractional digits only when present, 3/6/9 like `chrono`'s `AutoSi`), `parse_rfc3339()` (offsets, lower-case `t`/`z`, a space separator; a leap second `:60` rolls into the next minute), `to_rfc2822()` for mail `Date:` headers. UTC only, by design.
- **`uuid`** — `Uuid::new_v4()`, `parse` (hyphenated or simple, any case), `Display`, `simple()`, `NIL`.
- **`base64`** — `STANDARD`, `STANDARD_NO_PAD`, `URL_SAFE`, `URL_SAFE_NO_PAD` with `encode`/`decode`. Strict decoding: bad characters, padding and non-zero trailing bits are errors (verified against the `base64` crate on random and mutated input).
- **`hex`** — `encode`, `encode_upper`, `decode`.
- **`error`** — `Error`, `Result`, the `Context` trait (`.context()` / `.with_context()` on `Result` and `Option`) and the `err!`, `bail!`, `ensure!` macros. `{e}` prints the outermost message, `{e:#}` the whole chain, `{e:?}` a `Caused by:` list.
- **`env`** — `get_or`, `get_any`, `is_or` (an unset variable gives the default; set to anything except empty/`false`/`no`/`0` is true), plus `set`/`remove` for tests and startup code.
- **`map`** — `ShardedMap`, a concurrent map with closure-based access (`get`, `with`, `update`, `update_existing`, `remove_if`, `retain`, `for_each`). No guards, so holding one across another call cannot deadlock.
- **`version`** — `Version` and `VersionReq` with cargo's rules (`^`, `~`, `=`, `<`/`>`/`<=`/`>=`, `*` wildcards, comma-separated comparators, the pre-release rule). Checked against `semver` on thousands of generated requirements.
- **`bytes`** — `Bytes`, an immutable shareable buffer; clones and slices share storage.
- **`tmp`** — `TempDir`, removed on drop.
- **`future`** — `BoxFuture`, `LocalBoxFuture` and `block_on` (a tiny park-based executor for tests and tools; real I/O stays on tokio, which also supplies `select!`/`join!`).
- **`stream`** — `Stream`, `StreamExt` (`next`, `map`, `collect_vec`), `BoxStream`, and the sources `iter`, `once`, `empty`.
- **`async_trait`** — the `#[async_trait]` / `#[async_trait(?Send)]` attribute, for traits with `async fn` that are used as `dyn Trait`. Implemented in `quench-core-macros` (a proc-macro crate re-exported here); every elided lifetime in the arguments is named so the boxed future can borrow them. Traits that are only ever used generically can use native `async fn` instead.
- **`random`** — `fill`, `try_fill`, `bytes::<N>()`.
- **`registry`** — `Registry<T>` and the `submit!(REGISTRY, Type, value)` macro: items registered before `main` by a constructor the linker places in the platform's init section (`.init_array`, `__mod_init_func`, `.CRT$XCU`), so annotated routes and components need no hand-kept list. Iteration is newest first, as `inventory`'s was. An object file the linker never pulls in registers nothing - reference something from a library crate whose registrations you rely on. Not available on `wasm32`.

## On `wasm32`

`std` has no clock and no randomness there, so the host installs them once at startup: `quench_core::time::set_clock(fn() -> i128)` (nanoseconds since the epoch) and `quench_core::random::set_source(fn(&mut [u8]) -> bool)`. Until then `Time::now()` is the epoch and random calls fail.

## Testing

Unit tests live next to the code. RFC 4648 (base64), RFC 4122 (UUID) and RFC 3339/2822 vectors are covered; line coverage is ~99% for `quench-core` and ~96% for `quench-core-macros`, whose expansion logic is a plain function so the unit tests can reach it.

[Home](../README.md)
