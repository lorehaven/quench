---
title: Usage
parent: quench-core
grand_parent: Crates
nav_order: 1
---

Worked examples for the common tasks. Every item used here is documented in [API](api.md) and [API - system](api-system.md).

## 1. Application errors with context

Goal: return one error type from glue code and print the whole story.

```rust
use quench_core::{Context, Result, bail, ensure};

fn load(path: &str) -> Result<String> {
    ensure!(!path.is_empty(), "empty path");
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    if text.trim().is_empty() {
        bail!("{path} is empty");
    }
    Ok(text)
}

fn main() -> Result<()> {
    load("settings.toml")?;
    Ok(())
}
```

`{e}` prints `reading settings.toml`; `{e:#}` prints `reading settings.toml: No such file or directory (os error 2)`; returning `Err` from `main` prints the `Debug` form with a numbered `Caused by:` list. Pitfall: `quench_core::Error` is the derive macro; spell the type `quench_core::error::Error` (or use `quench_core::Result`).

## 2. A library error enum with `#[derive(Error)]`

```rust
use quench_core::Error;

#[derive(Debug, Error)]
enum LoadError {
    #[error("cannot read {path}: {source}")]
    Read { path: String, source: std::io::Error },
    #[error("bad value {0}")]
    Bad(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

See [quench-core-macros](../quench-core-macros/api.md) for every attribute.

## 3. Time: parse, add, format

```rust
use quench_core::time::{Duration, Time};

let t = Time::parse_rfc3339("2026-09-29T12:00:00+02:00")?; // 10:00:00Z
let later = t + Duration::minutes(90);
assert_eq!(later.to_rfc3339(), "2026-09-29T11:30:00Z");
assert_eq!(later.format("%a %d %b %Y"), "Tue 29 Sep 2026");
```

Pitfall: everything is UTC; for named zones use [quench-tz](../quench-tz/index.md). A `Date` parse failure reports `invalid RFC 3339 time: ...` because it shares `ParseTimeError`.

## 4. Dates and calendar arithmetic

```rust
use quench_core::date::Date;
let d = Date::from_ymd(2026, 1, 31).unwrap();
assert_eq!(d.add_months(1).unwrap().to_string(), "2026-02-28");
assert_eq!(Date::parse_from_str("29/09/2026", "%d/%m/%Y").unwrap().weekday().name(), "Tue");
```

## 5. IDs, tokens and encodings

```rust
use quench_core::{base32, base64, hex, random, uuid::Uuid};

let id = Uuid::new_v4();
let token: [u8; 32] = random::bytes();
let wire = base64::URL_SAFE_NO_PAD.encode(token);
assert_eq!(base64::URL_SAFE_NO_PAD.decode(&wire).unwrap(), token);
let secret = base32::encode(b"12345678901234567890");
println!("{id} {} {secret}", hex::encode(&token[..4]));
```

Pitfall: `base64` decoding is strict; two spellings of one value are an error, which is what you want when verifying signatures.

## 6. Load a `.env` at start-up

```rust
fn main() {
    // First thing, before any thread or runtime exists.
    if let Err(e) = quench_core::dotenv::load() {
        eprintln!("{e}");
    }
    let port: u16 = quench_core::env::get_parsed("PORT", 8080);
    let verbose = quench_core::env::is_or("VERBOSE", false);
    println!("{port} {verbose}");
}
```

Variables already set are kept. See [internals](internals.md) for why the call must precede any thread.

## 7. Walk a tree like `grep -r`

```rust
use quench_core::fs::Walk;
let files = Walk::new("src")
    .sort_by_file_name()
    .standard_filters()
    .into_iter()
    .filter_map(Result::ok)
    .filter(|e| e.file_type().is_file())
    .count();
```

Pitfall: errors arrive as `Err` items; `filter_map(Result::ok)` drops them silently.

## 8. A shared counter map

```rust
use quench_core::map::ShardedMap;
let counts: ShardedMap<&'static str, u64> = ShardedMap::new();
counts.update("hit", || 0, |n| *n += 1);
let total = counts.with("hit", |n| *n);
```

Do not call into the same map from inside a closure: the shard is locked while it runs.

## 9. Async without a runtime

```rust
use quench_core::future::{block_on, join_all};
let sums = block_on(join_all((1..=3).map(|n| async move { n * 2 })));
assert_eq!(sums, vec![2, 4, 6]);
```

`block_on` parks the thread; use tokio for real I/O.

## 10. Registering items before `main`

```rust
use quench_core::registry::Registry;
struct Route(&'static str);
static ROUTES: Registry<Route> = Registry::new();
quench_core::submit!(ROUTES, Route, Route("/health"));
```

Pitfall: a crate that nothing in the binary references is not linked in and registers nothing.
