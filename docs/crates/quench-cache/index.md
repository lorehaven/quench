---
title: quench-cache
parent: Crates
nav_order: 7
has_children: true
permalink: /crates/quench-cache/
---

`quench-cache` (library `quench_cache`) is the caching layer: an in-process cache by default and Redis (a single server or a cluster) behind the `redis` feature, with one API (`CacheStore`) over both so that the backend becomes configuration. It also holds `RequestCache`, a small TTL cache for request/response pairs. It is built on [quench-redis](../quench-redis/index.md); [quench-auth](../quench-auth/index.md) stores its sessions and JWKS in a `CacheStore`.

| | |
|---|---|
| Version | `0.1.20` |
| Edition | `2024` (workspace) |
| MSRV | not declared |
| Async | `CacheStore` methods are `async`; `DataCache` and `RequestCache` are synchronous |

## Install

```toml
[dependencies]
quench-cache = { version = "0.1", features = ["redis"] }
```

## Features

| Feature | Default | Enables |
|---|---|---|
| `data-cache` | yes | `DataCache`, `CacheEntry`, `CacheStats` (module `data_cache`), `CacheStore` (module `store`). |
| `request-cache` | yes | `RequestCache`, `CachedResponse`, `CacheStats` (module `request_cache`). |
| `tls` | yes | `rediss://` for the Redis store: `quench-redis?/tls`. No effect without `redis`. |
| `redis` | no | The `CacheStore::Redis` variant and `RedisStore`, via `quench-redis`; implies `data-cache`. Without it, `CacheStore::from_env` returns an error when `CACHE_URL`/`REDIS_URL` is set. |

`CacheError` and `Result` are always available.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | `Error` derive for `CacheError`, `env::var`, `map::ShardedMap` (the concurrent map under both in-process caches). |
| `quench-serde` | `Value` (the stored value type), JSON encoding for Redis, `Serialize`/`Deserialize` for `CachedResponse`. |
| `quench-redis` (optional, `redis`) | The Redis client. |

Dev-dependencies: `quench-test`, `quench-core`, `quench-redis`, `tokio`. No third-party crates.

## Where to go next

* [Usage](usage.md)
* [API: CacheStore and RedisStore](api-store.md)
* [API: DataCache and RequestCache](api-caches.md)
* [Configuration](configuration.md)
* [Errors](errors.md)
* [Internals](internals.md): key layout, atomicity, TTL behaviour of each backend
* [Testing](testing.md)

## Minimal example

```rust
use quench_cache::CacheStore;
use quench_serde::json;

#[tokio::main]
async fn main() -> quench_cache::Result<()> {
    let store = CacheStore::from_env("my-service").await?;   // Redis if CACHE_URL/REDIS_URL is set
    store.set("greeting", json!({ "text": "hello" }), Some(60)).await?;
    if let Some(value) = store.get("greeting").await? {
        println!("{value}");
    }
    Ok(())
}
```
