---
title: Usage
parent: quench-cache
grand_parent: Crates
nav_order: 1
---

## 1. Pick the backend from the environment

```rust
let store = quench_cache::CacheStore::from_env("billing").await?;
println!("cache: {}", store.topology());   // "in-process memory", "a single Redis server" or "a Redis cluster"
```

With neither `CACHE_URL` nor `REDIS_URL` set (or set to blank text) the store is in-process. Otherwise it connects to Redis, and a failure to connect is an error, not a fallback. Without the `redis` feature a configured URL is an error. The argument is the key prefix on Redis (keys are stored as `billing:<key>`) and is ignored in-process.

## 2. Store and read values, with a TTL

```rust
use quench_serde::json;
store.set("user:1", json!({"name": "Ann"}), Some(300)).await?;   // 300 seconds
store.set("flag", json!(true), None).await?;                      // no expiry
let user = store.get("user:1").await?;                            // Option<Value>
store.remove("user:1").await?;
```

Values are `quench_serde::Value`. In Redis they are stored as JSON text with `SET key json [EX ttl]`. Pitfall: a TTL of `0` expires immediately in-process but is rejected by Redis (the error comes back as `CacheError::Backend`).

## 3. Single-use tokens with `take`

```rust
store.set("reset:abc", json!("ann"), Some(900)).await?;
if let Some(user) = store.take("reset:abc").await? {   // read + delete
    // redeem
}
```

On Redis `take` is `GETDEL` and is atomic: of two racing callers exactly one gets the value. In-process it is a `get` followed by a `delete` and is not atomic across threads (see [Internals](internals.md#atomicity)).

## 4. Sets of members (several sessions per user)

```rust
store.add_to_set("user:ann", "sid-1", Some(3600)).await?;
store.add_to_set("user:ann", "sid-2", Some(3600)).await?;
let mut ids = store.set_members("user:ann").await?;   // Vec<String>, any order
store.remove_from_set("user:ann", "sid-1").await?;
```

Adding a member refreshes the set's lifetime to the given TTL (the TTL replaces the old one; `None` in-process makes it permanent, on Redis it leaves any existing expiry untouched). A missing set reads as empty.

## 5. Clear what you own

```rust
store.clear().await?;
```

On Redis this deletes keys matching `<prefix>:*` using `SCAN` (every primary on a cluster). In-process it empties the whole cache. With an empty prefix the pattern is `*`: the entire Redis database, so always give a prefix.

## 6. Use the in-process caches directly

```rust
use quench_cache::{DataCache, RequestCache};
use quench_cache::request_cache::CachedResponse;

let cache = DataCache::new();
cache.set_with_ttl("k".into(), quench_serde::json!(1), 60);
assert!(cache.has("k"));
let removed = cache.cleanup_expired();      // nothing expired yet

let requests = RequestCache::with_ttl(3600);
let key = RequestCache::generate_key(&["model-x", "the prompt"]);
requests.set(key.clone(), CachedResponse { content: "hi".into(), model: "model-x".into(), timestamp: 1_700_000_000, tokens_used: Some(12) });
```

Expired entries of `DataCache` are removed lazily on `get` or by `cleanup_expired`; `RequestCache` judges expiry from the `timestamp` field you store, not from the time of `set` (see [API](api-caches.md#requestcache)).

## 7. Connect to a cluster

```rust
use quench_cache::store::RedisStore;
let store = RedisStore::connect("redis://n1:7000,redis://n2:7000", "svc").await?;   // comma = cluster seeds
let one_seed = RedisStore::connect_with("redis://n1:7000", "svc", true).await?;       // force cluster mode
let store = quench_cache::CacheStore::Redis(store);
```

Or set `CACHE_URL=redis://n1:7000,redis://n2:7000` (or `CACHE_CLUSTER=true` with one seed) and call `from_env`.

## Pitfalls

* Keys of different services must not collide: give each service its own prefix.
* In-process entries are per process and lost on restart; check `is_shared()` before relying on cross-replica consistency.
* A value read from Redis that is not valid JSON is `CacheError::SerializationError`.
