# quench-cache
Caching: in-process by default, Redis behind the `redis` feature.

## What it is
`quench-cache` is the caching layer. `CacheStore` is one async API over an in-process cache and Redis (a single server or a cluster), so the backend becomes configuration. `DataCache` and `RequestCache` are the synchronous in-process caches; `RequestCache` is a small TTL cache for request/response pairs.

It is built on `quench-redis`, and `quench-auth` stores its sessions and JWKS in a `CacheStore`. Values are `quench_serde::Value`.

## Use cases
- Choose in-process or Redis storage from `CACHE_URL` / `REDIS_URL`.
- Cache values with a TTL.
- Redeem single-use tokens with `take`.
- Keep sets of members, such as several sessions per user.

## Example
```rust
use quench_serde::json;

let store = quench_cache::CacheStore::from_env("billing").await?;
println!("cache: {}", store.topology());

store.set("user:1", json!({"name": "Ann"}), Some(300)).await?;   // 300 seconds
let user = store.get("user:1").await?;                            // Option<Value>

store.set("reset:abc", json!("ann"), Some(900)).await?;
if let Some(user) = store.take("reset:abc").await? {              // read + delete
    // redeem
}

store.add_to_set("user:ann", "sid-1", Some(3600)).await?;
let ids = store.set_members("user:ann").await?;
```

## Features
- `data-cache` (default): `DataCache`, `CacheEntry`, `CacheStore`.
- `request-cache` (default): `RequestCache`, `CachedResponse`.
- `tls` (default): `rediss://` for the Redis store; no effect without `redis`.
- `redis`: the Redis backend via `quench-redis`; implies `data-cache`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-cache/
