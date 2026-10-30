# Gap report: foundations (quench-core, -core-macros, -serde, -serde-macros, -log, -log-macros, -config, -tz, -security)

> **Fixed 2026-10-30** (the pages of the book describe the fixed behaviour; the text below is the original finding):
> * Open question 1: TOML and YAML nesting is capped at 128 (`recursion limit exceeded`).
> * Open question 3: a `Date` parse failure now says `invalid date: ...`.


Written 2026-10-30 while replacing the flat pages with `docs/crates/<crate>/` directories and writing the guides `getting-started`, `architecture`, `dependency-policy`, `conventions`. Everything below was checked against the source; examples were built and run in a scratch crate with path dependencies (outside the repository). `cargo test --offline` of all nine crates passes (the `gate` and `fuzz` tests of quench-security were not run: they need the network and extra tools).

## 1. Stale or wrong statements in the old docs

| Old page | Said | Code does |
|---|---|---|
| `quench-core.md` | "Its only dependency is `getrandom`" | also depends on `quench-core-macros` (`Cargo.toml`), the crate that provides `#[derive(Error)]` and `#[async_trait]` |
| `quench-core.md` | the security gate "allows no other file to use [the `unsafe`]" | `quench-core/src/registry.rs:107,116,118` expands `unsafe(link_section = ...)` inside `#[cfg_attr]`; the line scanner (`quench-security/src/lib.rs`, `line_uses_unsafe`) does not match that spelling, so the file is not in `scripts/unsafe-allowlist.txt` |
| `quench-core.md` | `registry` "Not available on `wasm32`" | no `cfg(target_arch = "wasm32")` in `registry.rs` or the `submit!` macro; behaviour on wasm32 not verified |
| `quench-serde.md` | tuples implemented "up to 6" | `tuples!` in `traits.rs:579-592` goes to 12 elements |
| `quench-serde.md` | `Deserialize` for "`String`/`&str`/`char`" | no `Deserialize for &str`; `str` is `Serialize` only (`&T` delegates) |
| `quench-serde.md` | TOML/JSON parser description | the old text never mentions that JSON caps nesting at 128 but TOML and YAML have no cap (see section 3) |
| `quench-log.md` | JSON lines have `timestamp`, `level`, `target`, `message`, "the event's fields at the top level" and a `spans` array | `format::json` writes `{"timestamp","level","fields":{"message",...event fields},"target","span":{innermost},"spans":[...]}`; fields and message are nested under `fields`, and there is an extra `span` key |
| `quench-log.md` / `config.rs` doc comment | console output "to stderr" (comment on `Config::console_enabled`, `quench-log/src/config.rs`) | `ConsoleSink` writes to standard output (`dispatch.rs`, `std::io::stdout()`) |
| `quench-log/README.md` | "One `tracing` subscriber for every service" | `tracing` is not used; wording is a leftover |
| `quench-log-macros.md` | "`self` is never recorded unless you add it through `fields`" | `instrument.rs` records `self` (`self = ?self`) unless `skip(self)`; the unit test `methods_record_self` and `quench-log/tests/unit/instrument_tests.rs::methods_record_self` assert it |
| `quench-config.md` | files are read "via `serde_json`/`serde_norway`/`toml`"; "thiserror-derived" `ConfigError`; source at `libs/quench-config/src/lib.rs`, tests at `libs/quench-config/tests/...` | the loaders call `quench_serde::{json,yaml,toml}::from_str`; `ConfigError` uses `quench_core`'s `#[derive(Error)]`; the crate lives at `quench-config/` |
| `quench-config.md` | `InvalidValue` listed as a usable variant | declared but never constructed anywhere in the crate |
| `quench-serde-yaml.md` | "replacing `serde_norway`"; "Its only dependency is `quench-serde`" | the module is part of `quench-serde` (feature `yaml`), not a crate with dependencies of its own |
| `quench-tz.md` | "`data/zones.qtz` (152 KB)" | 151,994 bytes (about 148 KiB); version `2026e`, 598 names, 345 distinct rule bodies (verified by decoding the file) |
| `quench-core/src/env.rs:74` and `scripts/unsafe-allowlist.txt` (comment) | point to `docs/quench-core.md` for "the contract" | that page no longer exists; the contract is now in `docs/crates/quench-core/internals.md` (source and script are outside the documentation task and were not edited) |

## 2. In the code but undocumented before

* quench-core: `base32`, `date` (`Date`, `Civil`, `Weekday`), `bytes` API, the `ignore` module (`Gitignore`, `Ignorer`, `Match`), `Walk::standard_filters`, `Entry`/`WalkIter`, `FuturesUnordered`, `StreamExt::buffer_unordered`, `Registry`/`submit!` details, `time::set_clock`/`random::set_source`, error messages of every decode error, `Time::format` specifiers, `Duration` display rules.
* quench-core-macros: every compile-error message; the path-field `.display()` behaviour of `#[derive(Error)]`; the `.field` argument rewriting; the `Sync`/`Send` bound added for default bodies of `#[async_trait]` traits.
* quench-serde: `Value::pointer`, `ValueIndex`, `Map` API, `json::MAX_DEPTH`/`float_text`, `urlencoded::percent_encode`/`percent_decode`/`parse_pairs`, lenient (form) mode, the `u128`/`i128` behaviour, `Duration` shape `{secs, nanos}`, `Error::at/in_field/in_index`, the hidden `__private` module.
* quench-serde-macros: all attributes and their exact compile errors; the externally/internally/untagged shapes; unit-struct `null`; flatten of non-objects inserts `value`.
* quench-log: `field` module (`IntoFieldValue`, `Empty`, `display`, `debug`), `Sink`/`Dispatcher`/`MemorySink`, `RollingWriter`, `NonBlocking::dropped`, the 128,000-line queue, `span!` macro family, `format_text`/`format_json`/`timestamp`, filter syntax details (bare target turns a target fully on, `[` is truncated).
* quench-tz: `Database`, `pack`, `Rule`, `ZoneData`, `LocalType`, `Reading`, `TzifError` reasons, the QTZ1 binary format, `Zone::fixed` abbreviation rule.
* quench-security: the whole crate (it had no page): scanners, helpers, stages, fuzz harness, environment variables.
* Guides: nothing existed for getting started, architecture, dependency policy or conventions.

## 3. Open questions and suspected bugs

1. **TOML and YAML parsers have no nesting limit.** `toml::parse("a = " + "["*100000 + "]"*100000)` and `yaml::parse("["*100000 + "]"*100000)` abort with `fatal runtime error: stack overflow` (reproduced in a scratch program; 1000 levels are fine). `json::parse` returns `recursion limit exceeded` for the same depth. Files: `quench-serde/src/toml.rs`, `quench-serde/src/yaml/parse.rs`. Matters for untrusted input.
2. **`Walk::filter_entry` replaces the filter installed by `standard_filters`** (`quench-core/src/fs.rs:123-126`: `self.filter = Some(...)`), while `standard_filters` wraps an earlier filter. Order of the two calls changes whether ignore rules apply.
3. **`Date::parse_from_str` / `Date::from_str` errors are reported as `invalid RFC 3339 time: ...`** because they reuse `ParseTimeError` (`quench-core/src/date.rs:~132`, `time.rs:~300`). Misleading text for a date.
4. **Three different boolean parsers**: `quench_core::env::parse_bool` (anything except empty/`false`/`no`/`0` is true), `quench_log::config::parse_bool` (`Option<bool>`; unknown text keeps the default) and `quench_config::ConfigLoader::env_bool` (true only for `true 1 yes on`; any other present value is false). The same variable can mean different things depending on the reader.
5. **`quench-config` `load_with_fallback`** silently ignores an environment variable that is not valid JSON for `T` (`lib.rs:~79`, `if let Ok(config) = ...`), and a file with an unrecognised extension falls through to the environment without an error.
6. **Console sink uses stdout**, but the `Config::console_enabled` comment says stderr (see section 1); decide which is intended.
7. **`quench_tz::Rule::reading_at` returns a type that cannot be named**: `posix::Reading` is a second, private-module struct with the same fields as `zone::Reading`; only `zone::Reading` is re-exported (`quench-tz/src/lib.rs`, `posix.rs:41`).
8. **`quench-core` registry on wasm32**: the init-section attributes have no wasm branch; behaviour on `wasm32-unknown-unknown` is unspecified in the code.
9. **`line_uses_unsafe` is a line-pattern scan** and misses `unsafe(...)` inside `cfg_attr` (see section 1); the allowlist does not list `quench-core/src/registry.rs`.
10. **`quench-security` expects `scripts/secrets-allowlist.txt` to be optional** (`gate.rs` uses `unwrap_or_default`); it does not exist in the repository.
11. **`deny.toml`**: `unknown-registry = "warn"` and `unknown-git = "warn"` are warnings, not errors; the workspace crates come from the private registry `ennor`. The `[bans] allow` list currently equals the resolved third-party graph exactly (41 crates, verified with `cargo metadata --offline --all-features`).
12. **`anvil machete` / `anvil audit` / `anvil deny`** (`.conveyor.toml` job `deps`): the `anvil` source is not in this repository, so that they run `cargo-machete`, `cargo-audit` and `cargo-deny` is inferred from the names. There is no `[package.metadata.cargo-machete]` configuration in the repository.
13. **Verification claims in the old pages** (comparisons against `serde_json`, `toml`, `serde_norway`, `semver`, `shlex`, Python `zoneinfo`; 99% / 96% coverage figures) cannot be reproduced from the repository; they are carried into the new pages only as "old page states ...".
14. **Message style** differs between crates (`quench-config` capitalises messages, `quench-log`/`quench-core` do not); no rule is written down.
15. **No MSRV**: no `rust-version` in any manifest; edition 2024 implies Rust 1.85 or later.
16. `quench-serde` `Value` `From<u128>/<i128>` are not provided, while `Serialize for u128/i128` falls back to an inexact `Float` above 64 bits and `Deserialize` accepts only the 64-bit range.
