# quench-core
Dependency-free foundations: time, uuid, base64/hex, errors, env, collections, versions.

## What it is
`quench-core` is the base crate of the workspace: time, UUIDs, text encodings, a catch-all error type, environment and `.env` helpers, a concurrent map, version parsing, byte buffers, directory walking, futures and streams. Its only dependencies are `quench-core-macros` and, on native targets, `getrandom`.

It stands in for `chrono`, `uuid`, `base32`, `base64`, `anyhow`, `dashmap`, `semver`, `bytes`, `tempfile`, `dotenvy`, `futures-util`, `walkdir`, `which` and `shlex`, among others. `#[async_trait]` and `#[derive(Error)]` come from `quench-core-macros` and are re-exported from the root. Most of the crate is pure and builds for `wasm32-unknown-unknown`.

## Use cases
- Return one error type from glue code with context (`Context`, `bail!`, `ensure!`).
- Parse, add and format UTC times and calendar dates.
- Generate UUIDs and random tokens, and encode them as base64, base32 or hex.
- Parse semantic versions and requirements.
- Load a `.env` file, walk directories and create temporary directories.

## Example
```rust
use quench_core::Context;
use quench_core::time::Time;
use quench_core::uuid::Uuid;

fn run() -> quench_core::Result<String> {
    let id = Uuid::new_v4();
    let when = Time::parse_rfc3339("2026-09-29T12:00:00Z").context("bad timestamp")?;
    Ok(format!("{id} at {when}"))
}
```

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
```

The catch-all error type is `quench_core::error::Error`; the root `Error` is the derive macro.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-core/
