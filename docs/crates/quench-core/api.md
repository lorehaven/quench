---
title: API - errors, encodings, time, versions
parent: quench-core
grand_parent: Crates
nav_order: 2
---

Reference for the modules `error`, `env`, `base32`, `base64`, `hex`, `uuid`, `random`, `time`, `date` and `version`. The remaining modules are in [API - system and async helpers](api-system.md). Signatures are copied from `quench-core/src`.

## Crate root (`lib.rs`)

```rust
pub use error::{Context, Result};
pub use quench_core_macros::{Error, async_trait};
```

`quench_core::Error` is the derive macro; the error type is `quench_core::error::Error`. `Context` and `Result` are the trait and alias of `error`. See [quench-core-macros](../quench-core-macros/api.md).

## `error`

A catch-all error for application code (`anyhow`'s role). Library crates define their own enums (often with `#[derive(Error)]`); binaries and glue code use this type.

### `type Result`

```rust
pub type Result<T, E = Error> = std::result::Result<T, E>;
```

### `struct Error`

```rust
pub struct Error { /* contexts: Vec<String>, root: Box<dyn StdError + Send + Sync + 'static> */ }
```

Holds a root error and the context strings added on the way up (innermost first). `Send + Sync`. Does **not** implement `std::error::Error` (this is what allows the blanket `From<E>`).

| Item | Signature | Behaviour |
|---|---|---|
| `Error::msg` | `pub fn msg(message: impl Display) -> Self` | Error from a message alone; the root is a private message type. |
| `Error::new` | `pub fn new(error: impl StdError + Send + Sync + 'static) -> Self` | Wraps a concrete error, keeping it for `downcast_ref`. |
| `Error::context` | `#[must_use] pub fn context(mut self, context: impl Display) -> Self` | Pushes a context layer; the last one added is the outermost. |
| `Error::chain` | `#[must_use] pub fn chain(&self) -> Vec<String>` | Contexts from outermost to innermost, then the root's message, then each `source()` of the root in turn. |
| `Error::root_cause` | `#[must_use] pub fn root_cause(&self) -> &(dyn StdError + Send + Sync + 'static)` | The root error as stored. |
| `Error::downcast_ref` | `#[must_use] pub fn downcast_ref<E: StdError + 'static>(&self) -> Option<&E>` | The root as `E`, if it is one. Context layers do not hide it. |

Trait implementations:

* `impl<E: StdError + Send + Sync + 'static> From<E> for Error`: enables `?`.
* `Display`: the outermost context, or the root's message when there is no context. `{:#}` joins the whole `chain()` with `: `.
* `Debug`: the first chain element, then (if the chain has more) a blank line, `Caused by:` and one line per further element formatted `    {i}: {message}` with `i` from 0. This is what `main` prints when it returns `Err`.

```rust
use quench_core::Context;

let err = std::fs::read("missing").context("loading settings").unwrap_err();
assert_eq!(err.to_string(), "loading settings");
assert!(format!("{err:#}").starts_with("loading settings: "));
```

### `trait Context<T>`

```rust
pub trait Context<T> {
    fn context(self, context: impl Display) -> Result<T>;
    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T>;
}
```

Implemented for `std::result::Result<T, E>` where `E` is a `StdError + Send + Sync + 'static` **or** `Error` itself (through a private helper trait), and for `Option<T>`. For a `Result`, the error is wrapped (or, if already an `Error`, extended) with the context. For an `Option`, `None` becomes `Error::msg(context)`. `with_context` builds the text only on the failure path.

### Macros

All three are `#[macro_export]`, so they live at the crate root (`quench_core::err!`, `bail!`, `ensure!`).

| Macro | Forms | Expansion |
|---|---|---|
| `err!` | `err!("text")`, `err!("fmt {}", a)`, `err!(expr)` | `$crate::error::Error::msg(format!(...))` (the `expr` form passes the value to `Error::msg`). |
| `bail!` | same as `err!` | `return Err($crate::err!(...))`. |
| `ensure!` | `ensure!(cond)`, `ensure!(cond, "fmt", args...)` | When `!cond`, `bail!`s. Without a message the text is ``condition failed: `<stringified cond>` ``. |

`bail!` and `ensure!` return `Result<_, Error>` and so need a function whose error type `Error` converts into (typically `quench_core::Result`).

## `env`

Environment helpers. Boolean rule: a value is `true` unless it is empty, `false`, `no` or `0` (case-insensitive).

```rust
pub fn get_or(key: impl AsRef<OsStr>, default: &str) -> String
pub fn get_any<K: AsRef<OsStr>>(keys: &[K], default: &str) -> String
pub fn get_parsed<T: std::str::FromStr>(key: impl AsRef<OsStr>, default: T) -> T
pub fn is_or(key: impl AsRef<OsStr>, default: bool) -> bool
pub fn parse_bool(value: &str) -> bool
pub fn var_os(key: impl AsRef<OsStr>) -> Option<OsString>
pub fn var(key: impl AsRef<OsStr>) -> Option<String>
pub fn set(key: impl AsRef<OsStr>, value: impl AsRef<OsStr>)
pub fn remove(key: impl AsRef<OsStr>)
```

The first eight readers are `#[must_use]`.

| Function | Behaviour |
|---|---|
| `get_or` | The value, or `default` when unset or not valid UTF-8. |
| `get_any` | The value of the first of `keys` that is set, else `default`. |
| `get_parsed` | Parsed with `FromStr`; `default` when unset, not UTF-8 or unparsable. |
| `is_or` | `parse_bool` of the value; `default` when unset. |
| `parse_bool` | The rule above. |
| `var_os` | `std::env::var_os`, so readers go through this module. |
| `var` | `var_os` as text; `None` when unset or not UTF-8. |
| `set` | Sets a variable in the process environment. Contains the workspace's `unsafe` (see [internals](internals.md)); callers must run at start-up before any thread, or in tests on variables of their own. |
| `remove` | Removes a variable; same contract. |

## `base32`

RFC 4648 base32, alphabet `A-Z2-7`.

```rust
pub enum DecodeError { InvalidByte(usize), InvalidLength }   // Debug, Clone, Copy, PartialEq, Eq
#[must_use] pub fn encode(input: impl AsRef<[u8]>) -> String
pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError>
```

* `encode`: upper case, no padding.
* `decode`: ignores case, `=` padding, spaces and hyphens. `InvalidByte(offset)` for a character outside the alphabet; `InvalidLength` when the trailing group has 1, 3 or 6 leftover characters.
* Messages: `invalid base32 character at offset {n}`; `base32 string has an impossible length`. `DecodeError` implements `Display` and `std::error::Error`.

## `base64`

RFC 4648 base64 with a strict decoder.

```rust
pub struct Base64 { /* alphabet, padded */ }          // Debug, Clone, Copy
pub const STANDARD: Base64          // '+' '/' with '=' padding
pub const STANDARD_NO_PAD: Base64   // '+' '/' no padding
pub const URL_SAFE: Base64          // '-' '_' with padding
pub const URL_SAFE_NO_PAD: Base64   // '-' '_' no padding
pub enum DecodeError { InvalidByte(usize), InvalidLength, InvalidPadding, NonCanonical }
impl Base64 {
    #[must_use] pub fn encode(&self, input: impl AsRef<[u8]>) -> String
    pub fn decode(&self, input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError>
}
```

Decoding rejects characters outside the alphabet (`InvalidByte(offset)`), a dangling single character (`InvalidLength`), missing/extra/misplaced padding (`InvalidPadding`) and trailing bits an encoder would have left zero (`NonCanonical`). Messages: `invalid base64 byte at offset {n}`, `invalid base64 length`, `invalid base64 padding`, `non-canonical base64 trailing bits`. `DecodeError` implements `Display` and `Error`.

```rust
use quench_core::base64;
let text = base64::URL_SAFE_NO_PAD.encode(b"\xfb\xff");
assert_eq!(base64::URL_SAFE_NO_PAD.decode(&text).unwrap(), b"\xfb\xff");
```

## `hex`

```rust
pub enum DecodeError { OddLength, InvalidByte(usize) }
#[must_use] pub fn encode(input: impl AsRef<[u8]>) -> String        // lower case
#[must_use] pub fn encode_upper(input: impl AsRef<[u8]>) -> String
pub fn decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError>  // either case
```

Messages: `hex string has an odd length`, `invalid hex digit at offset {n}`.

## `uuid`

```rust
pub struct Uuid([u8; 16]);     // Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default
pub struct ParseUuidError(String);   // Debug, Clone, PartialEq, Eq
pub struct Simple(Uuid);       // Debug, Clone, Copy
```

| Item | Signature | Behaviour |
|---|---|---|
| `Uuid::NIL` | `pub const NIL: Self` | All zero. |
| `Uuid::new_v4` | `#[must_use] pub fn new_v4() -> Self` | 16 bytes from `random::bytes`, version nibble 4, variant bits `10`. Panics if the OS gives no randomness (via `random::fill`). |
| `Uuid::from_bytes` | `#[must_use] pub const fn from_bytes(bytes: [u8; 16]) -> Self` | |
| `Uuid::as_bytes` | `#[must_use] pub const fn as_bytes(&self) -> &[u8; 16]` | |
| `Uuid::is_nil` | `#[must_use] pub const fn is_nil(&self) -> bool` | |
| `Uuid::version` | `#[must_use] pub const fn version(&self) -> u8` | The version nibble. |
| `Uuid::simple` | `#[must_use] pub const fn simple(&self) -> Simple` | Displays as 32 lowercase hex digits without hyphens. |
| `Uuid::parse` | `pub fn parse(text: &str) -> Result<Self, ParseUuidError>` | Hyphenated 8-4-4-4-12 (length 36, hyphens at 8, 13, 18, 23) or 32 digits; any case; any version. |

`FromStr` (same as `parse`, `Err = ParseUuidError`), `Display` (hyphenated), `Debug`, and `Display for Simple`. `ParseUuidError` displays as `invalid UUID: {input}` and implements `Error`.

## `random`

```rust
pub struct RandomError;                                 // Debug, Clone, Copy, PartialEq, Eq
pub fn try_fill(buf: &mut [u8]) -> Result<(), RandomError>
pub fn fill(buf: &mut [u8])
#[must_use] pub fn bytes<const N: usize>() -> [u8; N]
#[cfg(target_arch = "wasm32")] pub fn set_source(source: fn(&mut [u8]) -> bool)
```

`try_fill` uses `getrandom::fill` natively; on `wasm32` it uses the source installed with `set_source` (the first call wins; the function fills the buffer and returns whether it could). `fill` and `bytes` panic with `OS randomness unavailable` when `try_fill` fails. `RandomError` displays `the operating system could not provide random bytes`.

## `time`

UTC only. `Duration` and `Time` hold `i128` nanosecond counts.

### `struct Duration`

```rust
pub struct Duration { /* nanos: i128 */ }   // Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default
```

| Item | Signature |
|---|---|
| `ZERO` | `pub const ZERO: Self` |
| constructors | `#[must_use] pub const fn nanoseconds(n: i64) -> Self`; likewise `microseconds`, `milliseconds`, `seconds`, `minutes`, `hours`, `days`, `weeks` (all `const fn`, argument `i64`) |
| `num_seconds` | `#[must_use] pub fn num_seconds(&self) -> i64` (floor, saturates to the `i64` range) |
| `num_milliseconds` | `#[must_use] pub fn num_milliseconds(&self) -> i64` (floor) |
| `is_negative` | `#[must_use] pub const fn is_negative(&self) -> bool` |
| `to_std` | `#[must_use] pub fn to_std(&self) -> Option<std::time::Duration>`; `None` when negative |
| `from_std` | `#[must_use] pub fn from_std(d: std::time::Duration) -> Self` |

Operators: `Add`, `Sub`, `Neg`, `Mul<i64>`. `Display` prints the largest non-zero units: `0s` for zero, a leading `-` for negative, then `{h}h`, `{m}m`, `{s}s` or `{s}.{fraction}s` with trailing zeros of the fraction trimmed (`-1h2m3.5s`).

### `enum Weekday`

```rust
pub enum Weekday { Mon, Tue, Wed, Thu, Fri, Sat, Sun }   // Debug, Clone, Copy, PartialEq, Eq, Hash
#[must_use] pub const fn name(self) -> &'static str      // "Mon", "Tue", ...
```

### `struct Civil`

Calendar fields in UTC. Derives `Debug, Clone, Copy, PartialEq, Eq`.

```rust
pub struct Civil {
    pub year: i32,
    pub month: u32,   // 1-12
    pub day: u32,     // 1-31
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub nanosecond: u32,
}
#[must_use] pub fn date(&self) -> crate::date::Date
#[must_use] pub fn to_time(&self) -> Option<Time>     // fields read as UTC; None when out of range
#[must_use] pub fn weekday(&self) -> Weekday
```

### `struct ParseTimeError`

```rust
pub struct ParseTimeError(String);   // Debug, Clone, PartialEq, Eq; Display, Error
```

`Display` is `invalid RFC 3339 time: {detail}`. The same type is returned by `Date::parse_from_str`, so a date error also begins with that prefix.

### `struct Time`

```rust
pub struct Time { /* nanos: i128 */ }   // Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default
```

| Item | Signature | Notes |
|---|---|---|
| `UNIX_EPOCH` | `pub const UNIX_EPOCH: Self` | |
| `now` | `#[must_use] pub fn now() -> Self` | System clock. On `wasm32` it comes from `set_clock`; until installed it is the epoch. |
| `from_unix` | `#[must_use] pub const fn from_unix(secs: i64) -> Self` | |
| `from_unix_millis` | `#[must_use] pub const fn from_unix_millis(millis: i64) -> Self` | |
| `from_unix_nanos` | `#[must_use] pub const fn from_unix_nanos(secs: i64, nanos: u32) -> Self` | `nanos` beyond one second carry into the seconds. |
| `from_ymd_hms` | `#[must_use] pub fn from_ymd_hms(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> Option<Self>` | `None` for out-of-range fields. |
| `timestamp` | `#[must_use] pub fn timestamp(&self) -> i64` | Seconds, floor. |
| `timestamp_millis` | `#[must_use] pub fn timestamp_millis(&self) -> i64` | |
| `subsec_nanos` | `#[must_use] pub fn subsec_nanos(&self) -> u32` | `0..1_000_000_000`. |
| `civil` | `#[must_use] pub fn civil(&self) -> Civil` | |
| `weekday` | `#[must_use] pub fn weekday(&self) -> Weekday` | |
| `to_rfc3339` | `#[must_use] pub fn to_rfc3339(&self) -> String` | `2026-09-29T12:00:00Z`; 3, 6 or 9 fractional digits only when sub-second precision exists. |
| `format` | `#[must_use] pub fn format(&self, pattern: &str) -> String` | `strftime`-style, UTC. |
| `to_rfc2822` | `#[must_use] pub fn to_rfc2822(&self) -> String` | `Tue, 29 Sep 2026 12:00:00 +0000`. |
| `parse_rfc3339` | `pub fn parse_rfc3339(text: &str) -> Result<Self, ParseTimeError>` | `YYYY-MM-DDTHH:MM:SS[.fraction](Z\|±HH:MM)`; `T` may be `t` or a space, `Z` may be `z`; digits beyond nanoseconds are dropped; `:60` rolls into the next minute. |

`format` specifiers: `%Y %y %m %d %e %H %M %S %j %a %A %b %B %F %T %s %f %% %Z %z`, plus `%3f` (milliseconds) and `%6f` (microseconds); `%f` is nine digits of nanoseconds. Unknown text is copied as written.

Traits: `From<SystemTime>`, `FromStr` (`Err = ParseTimeError`, same as `parse_rfc3339`), `Display` (RFC 3339), `Debug` (`Time(<rfc3339>)`), `Add<Duration>`, `Sub<Duration>`, `Sub<Time, Output = Duration>`, `AddAssign<Duration>`, `SubAssign<Duration>`.

### `set_clock` (wasm32 only)

```rust
#[cfg(target_arch = "wasm32")]
pub fn set_clock(now: fn() -> i128)
```

`now` returns nanoseconds since the Unix epoch. The first call wins.

```rust
use quench_core::time::{Duration, Time};
let t = Time::from_ymd_hms(2026, 9, 29, 12, 0, 0).unwrap();
assert_eq!((t + Duration::hours(1)).to_rfc3339(), "2026-09-29T13:00:00Z");
assert_eq!(t.format("%Y-%m-%d"), "2026-09-29");
```

## `date`

```rust
pub struct Date { /* days since 1970-01-01: i64 */ }   // Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default
```

| Item | Signature | Notes |
|---|---|---|
| `EPOCH` | `pub const EPOCH: Self` | 1970-01-01 (also `Default`). |
| `from_ymd` | `#[must_use] pub fn from_ymd(year: i32, month: u32, day: u32) -> Option<Self>` | `None` if the date does not exist. |
| `days_in_month` | `#[must_use] pub const fn days_in_month(year: i32, month: u32) -> u32` | month 1-12. |
| `from_days_since_epoch` | `#[must_use] pub const fn from_days_since_epoch(days: i64) -> Self` | |
| `days_since_epoch` | `#[must_use] pub const fn days_since_epoch(self) -> i64` | |
| `ymd` | `#[must_use] pub fn ymd(self) -> (i32, u32, u32)` | |
| `year`, `month`, `day` | `#[must_use] pub fn year(self) -> i32` / `month(self) -> u32` / `day(self) -> u32` | |
| `weekday` | `#[must_use] pub fn weekday(self) -> Weekday` | |
| `day_of_year` | `#[must_use] pub fn day_of_year(self) -> u32` | 1 for 1 January. |
| `add_days` | `#[must_use] pub fn add_days(self, days: i64) -> Option<Self>` | `None` past the representable range. |
| `add_months` | `#[must_use] pub fn add_months(self, months: i32) -> Option<Self>` | Clamps to the last day of a shorter month. |
| `days_since` | `#[must_use] pub const fn days_since(self, earlier: Self) -> i64` | |
| `at` | `#[must_use] pub fn at(self, hour: u32, minute: u32, second: u32) -> Civil` | Fields only; nanosecond 0, no range check. |
| `parse_from_str` | `pub fn parse_from_str(text: &str, pattern: &str) -> Result<Self, ParseTimeError>` | Pattern may use `%Y` (exactly four digits), `%m`, `%d` (up to two digits each) and literal characters; all three are required. |

`FromStr` parses `%Y-%m-%d`; `Display` prints `YYYY-MM-DD` (`{y:04}-{m:02}-{d:02}`); `Debug` prints `Date(YYYY-MM-DD)`.

## `version`

Semantic versions and cargo-style requirements.

```rust
pub struct ParseError(String);     // Debug, Clone, PartialEq, Eq; Display, Error
pub struct Version { pub major: u64, pub minor: u64, pub patch: u64, pub pre: String, pub build: String }  // Debug, Clone, PartialEq, Eq, Hash
pub struct VersionReq { /* comparators */ }                                                                // Debug, Clone, PartialEq, Eq, Hash
```

| Item | Signature | Notes |
|---|---|---|
| `Version::new` | `#[must_use] pub const fn new(major: u64, minor: u64, patch: u64) -> Self` | `pre` and `build` empty. |
| `Version::parse` | `pub fn parse(text: &str) -> Result<Self, ParseError>` | `major.minor.patch[-pre][+build]`. |
| `VersionReq::STAR` | `pub const STAR: Self` | Matches every release version. |
| `VersionReq::parse` | `pub fn parse(text: &str) -> Result<Self, ParseError>` | `*`, `x`, `X` give `STAR`; otherwise comma-separated comparators. |
| `VersionReq::matches` | `#[must_use] pub fn matches(&self, version: &Version) -> bool` | All comparators must hold; a pre-release version matches only if some comparator names a pre-release of the same `major.minor.patch`. |

`Version`: `FromStr`, `Display`, `Ord`/`PartialOrd` (semver ordering: a release outranks its pre-releases; identifiers compared numerically and below text, a prefix is smaller; build metadata is compared last, identifier-wise). `VersionReq`: `FromStr`, `Display`.

Comparator operators: `>=`, `<=`, `>`, `<`, `=`, `~`, `^`; a bare `1.2` means `^1.2`; `1.*` and `1.2.*` are wildcards when no operator is given; a field after a wildcard (`1.*.3`) is invalid; a pre-release needs a full `major.minor.patch`; build metadata in a requirement is invalid. Parse errors read `invalid version: "<text>"` or `invalid version requirement: "<text>"`.
