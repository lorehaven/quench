# Quench Core

`quench-core` (crate `quench_core`) is the dependency-free base every other quench crate builds on. It replaces `chrono`, `uuid`, `base64`, `anyhow`, `envmnt`, `dashmap`, `semver`, `bytes`, `tempfile`, `futures-util`, `async-trait`, `inventory`, `dotenvy`, `walkdir`, `which` and `shlex`. Its only dependency is `getrandom`, and only on native targets: the pure modules build for `wasm32-unknown-unknown`, which is what lets shared types compile on both the server and the browser side.

## Modules

- **`time`** — `Time` (a UTC instant, nanosecond resolution) and `Duration` (signed). `Time::now()`, `from_unix*`, `from_ymd_hms`, `civil()` (calendar fields), `weekday()`, `+`/`-` with `Duration`, `Time - Time`. Text: `to_rfc3339()` (also `Display`; fractional digits only when present, 3/6/9 like `chrono`'s `AutoSi`), `parse_rfc3339()` (offsets, lower-case `t`/`z`, a space separator; a leap second `:60` rolls into the next minute), `to_rfc2822()` for mail `Date:` headers, `format("%Y-%m-%d %H:%M:%S")` for a `strftime`-style rendering (`%Y %y %m %d %e %H %M %S %j %a %A %b %B %F %T %s %f %3f %6f %Z %z %%`; anything else is copied as written). `From<SystemTime>` converts file timestamps. UTC only, by design: there is no local time and no time zone database.
- **`uuid`** — `Uuid::new_v4()`, `parse` (hyphenated or simple, any case), `Display`, `simple()`, `NIL`.
- **`base32`** — RFC 4648 base32 (`A-Z2-7`): `encode` (upper case, no padding) and `decode` (any case; `=`, spaces and hyphens ignored), what authenticator apps show for a TOTP secret.
- **`base64`** — `STANDARD`, `STANDARD_NO_PAD`, `URL_SAFE`, `URL_SAFE_NO_PAD` with `encode`/`decode`. Strict decoding: bad characters, padding and non-zero trailing bits are errors (verified against the `base64` crate on random and mutated input).
- **`hex`** — `encode`, `encode_upper`, `decode`.
- **`error`** — `Error`, `Result`, the `Context` trait (`.context()` / `.with_context()` on `Result` and `Option`) and the `err!`, `bail!`, `ensure!` macros. `{e}` prints the outermost message, `{e:#}` the whole chain, `{e:?}` a `Caused by:` list.
- **`env`** — `get_or`, `get_any`, `is_or` (an unset variable gives the default; set to anything except empty/`false`/`no`/`0` is true), `get_parsed::<T>(key, default)` (any `FromStr`; unset or unparsable gives the default), plus `set`/`remove` for tests and startup code.
- **`map`** — `ShardedMap`, a concurrent map with closure-based access (`get`, `with`, `update`, `update_existing`, `remove_if`, `retain`, `for_each`). No guards, so holding one across another call cannot deadlock.
- **`version`** — `Version` and `VersionReq` with cargo's rules (`^`, `~`, `=`, `<`/`>`/`<=`/`>=`, `*` wildcards, comma-separated comparators, the pre-release rule). Checked against `semver` on thousands of generated requirements.
- **`bytes`** — `Bytes`, an immutable shareable buffer; clones and slices share storage.
- **`tmp`** — `TempDir`, removed on drop.
- **`fs`** — `Walk::new(root)`: recursive directory walk, directories before their contents; `sort_by_file_name`, `max_depth`, `follow_links` (a link back to an ancestor is an error entry, not an endless walk), `filter_entry` (prunes a directory with its contents). Yields `io::Result<Entry>` (`path`, `file_name`, `file_type`, `depth`, `metadata`); an unreadable directory is an entry followed by the error. Order, depth and link handling match `walkdir` on the real trees tried.
- **`process`** — `which(name)`: the executable a name resolves to on `PATH` (or a path as given), with the execute bit checked on Unix and `PATHEXT` on Windows.
- **`shell`** — `split` (POSIX word splitting: quotes, escapes, `#` comments; `None` for an open quote), `quote` and `join` (the inverse). Identical to `shlex::split` on 300k random inputs.
- **`dotenv`** — `parse` / `load_from(path)` / `load()` for `.env` files: `KEY=value`, `export`, comments, `'literal'` and `"escaped"` values (multi-line allowed), `$VAR` / `${VAR}` expansion; variables that are already set are never overridden; `load` looks in the current directory and its parents. Where `dotenvy` and this disagree it is on `dotenvy`'s own bugs (`$A_B` expanded as `$A` + `_B`, `"$X"` rejected).
- **`future`** — `join_all` (all at once, outputs in order), `catch_unwind` (a panic inside the future becomes an `Err` with the payload; wrap a non-`UnwindSafe` future in `AssertUnwindSafe`), `BoxFuture`, `LocalBoxFuture` and `block_on` (a tiny park-based executor for tests and tools; real I/O stays on tokio, which also supplies `select!`/`join!`).
- **`stream`** — `Stream`, `StreamExt` (`next`, `map`, `filter_map`, `chain`, `collect_vec`), `BoxStream`, and the sources `iter`, `once`, `empty` and `unfold(state, step)`, `FuturesUnordered` (a set of futures yielding outputs as they finish) and `StreamExt::buffer_unordered(n)` (run at most `n` of a stream's futures at once) (a stream built from an async step, which is how a channel receiver becomes a stream: `unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })`).
- **`async_trait`** — the `#[async_trait]` / `#[async_trait(?Send)]` attribute, for traits with `async fn` that are used as `dyn Trait`. Implemented in `quench-core-macros` (a proc-macro crate re-exported here); every elided lifetime in the arguments is named so the boxed future can borrow them. Traits that are only ever used generically can use native `async fn` instead.
- **`random`** — `fill`, `try_fill`, `bytes::<N>()`.
- **`registry`** — `Registry<T>` and the `submit!(REGISTRY, Type, value)` macro: items registered before `main` by a constructor the linker places in the platform's init section (`.init_array`, `__mod_init_func`, `.CRT$XCU`), so annotated routes and components need no hand-kept list. Iteration is newest first, as `inventory`'s was. An object file the linker never pulls in registers nothing - reference something from a library crate whose registrations you rely on. Not available on `wasm32`.

## On `wasm32`

`std` has no clock and no randomness there, so the host installs them once at startup: `quench_core::time::set_clock(fn() -> i128)` (nanoseconds since the epoch) and `quench_core::random::set_source(fn(&mut [u8]) -> bool)`. Until then `Time::now()` is the epoch and random calls fail.

## Testing

Unit tests live next to the code. RFC 4648 (base64), RFC 4122 (UUID) and RFC 3339/2822 vectors are covered; line coverage is ~99% for `quench-core` and ~96% for `quench-core-macros`, whose expansion logic is a plain function so the unit tests can reach it.

[Home](../README.md)
