---
title: quench-redis
parent: Crates
nav_order: 38
has_children: true
permalink: /crates/quench-redis/
---

`quench-redis` (library `quench_redis`) is Quench's Redis client. It replaces the `redis` crate with a RESP2/RESP3 codec, pooled connections that reconnect on demand, pipelines and Redis Cluster routing (CRC16 slots, `MOVED`/`ASK`). [quench-cache](../quench-cache/index.md) is its user in this workspace.

| | |
|---|---|
| Version | `0.1.8` |
| Edition | `2024` (workspace) |
| MSRV | not declared (`rust-version` is not set in the manifest) |
| Runtime | `tokio` only; there is no blocking API |

## Install

```toml
[dependencies]
quench-redis = "0.1"                       # TLS on (default)
# quench-redis = { version = "0.1", default-features = false }   # plain redis:// only
```

The workspace itself declares the dependency with `default-features = false`, so a workspace crate gets `rediss://` support only when it enables `tls` (as `quench-cache` does through its own `tls` feature).

## Features

| Feature | Default | Effect |
|---|---|---|
| `tls` | yes | Enables `rediss://` through `quench-tls` (rustls). Without it a configuration with TLS on fails in `Connection::connect` with `Error::Tls("quench-redis was built without its `tls` feature")` instead of silently connecting in plain text. |

## Dependencies

| Crate | Why |
|---|---|
| `tokio` (`io-util`, `net`, `sync`, `time`) | Sockets, `AsyncRead`/`AsyncWrite`, the pool's `Semaphore`, timeouts. |
| `quench-tls` (optional, `tls`) | TLS client configuration and connector for `rediss://`. |
| `quench-core` | Declared as a regular dependency but not referenced by anything under `src/` (the tests use `quench_core::uuid`). |

Dev-dependencies: `quench-test` (the test harness) and `tokio` with `macros`, `rt`, `rt-multi-thread`. There are no third-party dependencies beyond `tokio`.

## Modules and top-level items

| Item | Kind | Page |
|---|---|---|
| `Redis` | enum, one server or a cluster | [api-client.md](api-client.md) |
| `Client`, `PoolOptions`, `PooledConnection` | pool | [api-client.md](api-client.md) |
| `ClusterClient`, `SlotMap`, `SLOTS`, `crc16`, `key_slot`, `parse_primary` | cluster | [api-client.md](api-client.md) |
| `Config` | connection settings and URL | [api-config-and-commands.md](api-config-and-commands.md) |
| `Cmd`, `cmd`, `ToArg`, `FromValue` | commands and replies | [api-config-and-commands.md](api-config-and-commands.md) |
| `Connection`, `Stream` | one connection | [api-config-and-commands.md](api-config-and-commands.md) |
| `Error`, `Result`, `ServerError` | errors | [errors.md](errors.md) |
| `resp` (`Value`, `encode_command`, `decode`) | codec | [api-resp.md](api-resp.md) |

## Where to go next

* [Usage](usage.md): worked examples.
* [API: client, pool, cluster](api-client.md), [API: configuration, commands, connection](api-config-and-commands.md), [API: RESP codec](api-resp.md).
* [Configuration](configuration.md): the URL format and every default.
* [Errors](errors.md).
* [Internals](internals.md): retry rules, limits, routing.
* [Testing](testing.md).

## Minimal example

```rust
use quench_redis::{Redis, cmd};

#[tokio::main]
async fn main() -> Result<(), quench_redis::Error> {
    let redis = Redis::connect("redis://localhost:6379").await?;
    redis.exec::<()>(&cmd("SET").arg("greeting").arg("hello")).await?;
    let greeting: Option<String> = redis.exec(&cmd("GET").arg("greeting")).await?;
    assert_eq!(greeting.as_deref(), Some("hello"));
    Ok(())
}
```

`Redis::connect` opens one connection immediately, so a bad URL, an unreachable server or a wrong password fails here and not on the first command.
