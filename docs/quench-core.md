# Quench Core

`quench-core` (crate `quench_core`) is the dependency-free base every other quench crate builds on. It replaces `chrono` (UTC; zones are in [Quench TZ](./quench-tz.md)), `uuid`, `base64`, `anyhow`, `envmnt`, `dashmap`, `semver`, `bytes`, `tempfile`, `futures-util`, `async-trait`, `inventory`, `dotenvy`, `walkdir`, `which` and `shlex`. Its only dependency is `getrandom`, and only on native targets: the pure modules build for `wasm32-unknown-unknown`, which is what lets shared types compile on both the server and the browser side.

## Modules

- **`time`** — `Time` (a UTC instant, nanosecond resolution) and `Duration` (signed). `Time::now()`, `from_unix*`, `from_ymd_hms`, `civil()` (calendar fields), `weekday()`, `+`/`-` with `Duration`, `Time - Time`. Text: `to_rfc3339()` (also `Display`; fractional digits only when present, 3/6/9 like `chrono`'s `AutoSi`), `parse_rfc3339()` (offsets, lower-case `t`/`z`, a space separator; a leap second `:60` rolls into the next minute), `to_rfc2822()` for mail `Date:` headers, `format("%Y-%m-%d %H:%M:%S")` for a `strftime`-style rendering (`%Y %y %m %d %e %H %M %S %j %a %A %b %B %F %T %s %f %3f %6f %Z %z %%`; anything else is copied as written). `From<SystemTime>` converts file timestamps. UTC only, by design: there is no local time and no time zone database.
- **`date`** — `Date`, a calendar day without a time or zone: `from_ymd`, `ymd()`, `weekday()`, `day_of_year()`, `add_days`, `add_months` (clamps to the last day of a shorter month), `days_since`, `at(h, m, s)` (calendar fields), ISO text and `parse_from_str` for `%Y %m %d`. `Civil` also gained `date()`, `to_time()` and `weekday()`.
- **`uuid`** — `Uuid::new_v4()`, `parse` (hyphenated or simple, any case), `Display`, `simple()`, `NIL`.
- **`base32`** — RFC 4648 base32 (`A-Z2-7`): `encode` (upper case, no padding) and `decode` (any case; `=`, spaces and hyphens ignored), what authenticator apps show for a TOTP secret.
- **`base64`** — `STANDARD`, `STANDARD_NO_PAD`, `URL_SAFE`, `URL_SAFE_NO_PAD` with `encode`/`decode`. Strict decoding: bad characters, padding and non-zero trailing bits are errors (verified against the `base64` crate on random and mutated input).
- **`hex`** — `encode`, `encode_upper`, `decode`.
- **`error`** — `Error`, `Result`, the `Context` trait (`.context()` / `.with_context()` on `Result` and `Option`) and the `err!`, `bail!`, `ensure!` macros. `{e}` prints the outermost message, `{e:#}` the whole chain, `{e:?}` a `Caused by:` list.
- **`env`** — `get_or`, `get_any`, `is_or` (an unset variable gives the default; set to anything except empty/`false`/`no`/`0` is true), `get_parsed::<T>(key, default)` (any `FromStr`; unset or unparsable gives the default), plus `var`/`var_os` and `set`/`remove` for tests and startup code (see below).
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

## The environment and its one `unsafe`

`env::set` and `env::remove` change the real environment of the process: `std::env::var`, child processes and code in other libraries all see the change. That is what `.env` loading has to do, since services and libraries read their settings with `std::env::var`. They are also the only `unsafe` in the workspace's libraries that has no alternative: `std` marks `set_var` and `remove_var` `unsafe` (edition 2024) because on Unix the environment is a global C array, and changing it while another thread reads it (`getenv`, which libc itself calls for `TZ`, `LANG` and the resolver) is undefined behaviour. A design with an in-process overlay or a re-exec after loading was tried and rejected: the first leaves every `std::env` reader blind to `.env` values, the second restarts the process.

**The contract**, which the callers keep and the code cannot check:

- **Startup.** `dotenv::load`, `dotenv::load_from` and the `.env` loader of `quench-starter` (`logging::init`) run first thing in `main`, before the program has started a thread. A service must call `quench_starter::logging::init()` before it starts a runtime or a thread.
- **Tests.** The harness runs tests on threads. A test that calls `set` or `remove` uses variables nothing else reads (name them for the test), and the workspace's libraries read the environment only when asked, never from a background thread.
- **Nothing else.** Code that wants to pass a setting to other code passes a value. Code that starts a process sets the variable on the `Command`.

**How it is tested** (`quench-core/src/env.rs`, `dotenv.rs`, `quench-starter/src/logging.rs`): a variable set is seen by `std::env::var`, `var`, `var_os` and the getters; replacing and removing work, including removing what was never set; a child process (`sh -c`) inherits what was set and sees a removed variable as unset; a value that is not UTF-8 survives for `var_os` and is hidden from `var`; a loaded `.env` file is visible to plain `std::env` and to a child, and never overrides a variable that was already set. The `unsafe` is confined to these two functions, each with its `SAFETY` note, and the security gate allows no other file to use it (`scripts/unsafe-allowlist.txt`).
