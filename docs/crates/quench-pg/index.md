---
title: quench-pg
parent: Crates
nav_order: 34
has_children: true
permalink: /crates/quench-pg/
---

`quench-pg` (library `quench_pg`) is a PostgreSQL client written against `tokio`: the v3 wire protocol over TCP, Unix sockets or TLS, SCRAM-SHA-256 authentication, parameterised queries with typed rows, transactions and a connection pool. It replaces `sqlx`/`tokio-postgres` in the workspace. Rows are read through `quench-serde`, which is how [quench-db](../quench-db/index.md) maps models without a derive.

* Version: `0.1.9` (`quench-pg/Cargo.toml`). Edition `2024` (workspace); no `rust-version` is declared. The code uses `Duration::from_mins`, so it needs a toolchain where that is stable (Rust 1.91 or later).
* The library target has `harness = false` and its unit tests run under `quench-test` (`quench_test::main!()` in `src/lib.rs`).
* No `unsafe`.
* Everything travels in PostgreSQL's **text** format. There is no prepared-statement cache, no `COPY`, no `LISTEN` support (notifications are logged and dropped), no binary format.

## Install

```toml
[dependencies]
quench-pg = "0.1"
```

Without TLS:

```toml
[dependencies]
quench-pg = { version = "0.1", default-features = false }
```

## Features

| Feature | Default | Effect |
|---|---|---|
| `tls` | yes | `dep:quench-tls`. Enables `sslmode=require` and `verify-full`. Without it those modes fail with `Error::Tls("quench-pg was built without its `tls` feature")` before anything is sent, `disable` is unchanged and `prefer` (the default) stays on a plain connection without sending an `SSLRequest`. |

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `base64::STANDARD`, `hex`, `random::bytes`, `time::Time`, `uuid::Uuid`. |
| `quench-crypto` | SHA-256, HMAC, PBKDF2, constant-time compare for SCRAM. |
| `quench-log` | `debug!` for notices and notifications. |
| `quench-serde` | `Value`, `Deserialize`, JSON parsing/printing. |
| `quench-tls` (optional, feature `tls`) | TLS client (`ClientOptions`, `client_config`, `server_name`, `TlsConnector`). |
| `tokio` (features `io-util`, `net`, `sync`, `time`) | Sockets, I/O traits, timeouts, the pool's semaphore. |
| dev: `quench-test`, `tokio` (`macros`, `rt`, `rt-multi-thread`) | Tests. |

Third-party packages (`cargo tree -p quench-pg -e normal`, offline): 26 package versions with default features, 18 without `tls`. They come through `tokio` (`bytes`, `mio`, `socket2`, `libc`, `signal-hook-registry`, ...), `quench-crypto` (`ring`, `getrandom`) and `quench-tls` (`rustls`, `rustls-webpki`, `rustls-pki-types`, `subtle`).

## Where to go next

* [Usage](usage.md)
* API reference: [Config and connections](api-connection.md), [pool](api-pool.md), [rows](api-rows.md), [types](api-types.md), [wire protocol and SCRAM](api-protocol.md)
* [Configuration](configuration.md): URL, `Config`, `PoolOptions`
* [Errors](errors.md), [Internals](internals.md), [Testing](testing.md)

## Minimal example

```rust
use quench_pg::Pool;

async fn demo() -> Result<(), quench_pg::Error> {
    let pool = Pool::connect("postgres://app:secret@localhost/app").await?;
    let rows = pool.query("SELECT id, name FROM users WHERE name = $1", &[&"ann"]).await?;
    for row in rows {
        let (id, name): (i32, String) = (row.get("id")?, row.get("name")?);
        println!("{id} {name}");
    }
    Ok(())
}
```

This is the example in the crate documentation (`src/lib.rs`, `no_run`).
