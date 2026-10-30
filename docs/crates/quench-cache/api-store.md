---
title: "API: CacheStore and RedisStore"
parent: quench-cache
grand_parent: Crates
nav_order: 3
---

Items of `quench_cache` (crate root) and its `store` module. Crate root:

```rust
pub enum CacheError { Miss(String), SerializationError(String), Backend(String) }   // see errors.md
pub type Result<T> = std::result::Result<T, CacheError>;

#[cfg(feature = "data-cache")]   pub mod data_cache;
#[cfg(feature = "request-cache")] pub mod request_cache;
#[cfg(feature = "data-cache")]   pub mod store;
// re-exports (each behind its feature): DataCache, RequestCache, CacheStore
pub mod prelude { /* pub use DataCache, RequestCache, CacheStore (each behind its feature) */ }
```

`RedisStore` is **not** re-exported at the root or in the prelude: use `quench_cache::store::RedisStore`.

## `CacheStore`

```rust
#[derive(Clone, Debug)]
pub enum CacheStore {
    InMemory(Arc<crate::data_cache::DataCache>),   // per process
    #[cfg(feature = "redis")]
    Redis(RedisStore),                               // shared
}
```

Cheap to clone; clones share the same data. All methods are `async` and take `&self`; the in-process variant never actually awaits. The error type is always `CacheError`.

```rust
#[must_use] pub fn in_memory() -> Self
```
A fresh in-process store (`CacheStore::InMemory(Arc::new(DataCache::new()))`).

```rust
pub async fn from_env(key_prefix: impl Into<String>) -> Result<Self, CacheError>
```
Reads `CACHE_URL`, then `REDIS_URL` (first one set to valid UTF-8; blank or whitespace-only counts as unset). No URL: `in_memory()`. With a URL and the `redis` feature: `RedisStore::connect_with(url, key_prefix, forced)` where `forced` is true when `CACHE_CLUSTER` or `REDIS_CLUSTER` (first one set) equals `true` ignoring case and surrounding whitespace. Without the `redis` feature a URL gives `CacheError::Backend("a cache URL is configured (<url>) but quench-cache was built without the `redis` feature")`. `key_prefix` is unused in-process.

```rust
pub async fn get(&self, key: &str) -> Result<Option<Value>, CacheError>
pub async fn set(&self, key: &str, value: Value, ttl_secs: Option<u64>) -> Result<(), CacheError>
pub async fn take(&self, key: &str) -> Result<Option<Value>, CacheError>
pub async fn remove(&self, key: &str) -> Result<(), CacheError>
```
`get` returns `None` for a missing or expired key. `set` overwrites; `ttl_secs` is seconds, `None` means no expiry. `take` reads and removes (atomic on Redis only). `remove` is not an error when the key is absent. `Value` is `quench_serde::Value`.

```rust
pub async fn add_to_set(&self, key: &str, member: &str, ttl_secs: Option<u64>) -> Result<(), CacheError>
pub async fn set_members(&self, key: &str) -> Result<Vec<String>, CacheError>
pub async fn remove_from_set(&self, key: &str, member: &str) -> Result<(), CacheError>
```
Set semantics for members written concurrently. `add_to_set` is idempotent; with `Some(ttl)` the set's lifetime is refreshed. `set_members` returns members in no defined order, empty for a missing set. `remove_from_set` ignores a missing set or member; the key disappears with its last member (Redis removes empty sets itself, in-process drops the entry).

```rust
pub async fn clear(&self) -> Result<(), CacheError>
```
In-process: removes every entry. Redis: deletes the keys matching `<prefix>:*` (see `RedisStore::clear`).

```rust
#[must_use] pub const fn is_shared(&self) -> bool
#[must_use] pub const fn topology(&self) -> &'static str
```
`is_shared` is `false` for `InMemory`, `true` for `Redis`. `topology` returns `"in-process memory"`, `"a Redis cluster"` or `"a single Redis server"`, for startup logs.

## `RedisStore` (feature `redis`)

```rust
#[derive(Clone)]
pub struct RedisStore { /* redis: quench_redis::Redis, prefix: String */ }
```

`Debug` prints the prefix only. Every key is stored as `<prefix>:<key>` (or just `<key>` when the prefix is empty). Redis errors become `CacheError::Backend(err.to_string())`.

```rust
pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, CacheError>
pub async fn connect_with(url: &str, prefix: impl Into<String>, cluster: bool) -> Result<Self, CacheError>
```
`url` is split at commas, trimmed, empty pieces dropped. No pieces: `Backend("no Redis URL configured")`. One piece and `cluster == false`: `Redis::connect` (error text `Redis connection failed: ..`). More than one piece, or `cluster == true`: `Redis::connect_cluster` (error text `Redis cluster connection failed: ..`). Connecting opens a connection immediately, so a bad URL, password or topology fails here. `connect` is `connect_with(url, prefix, false)`.

```rust
#[must_use] pub const fn is_clustered(&self) -> bool
```

The remaining methods have the same signatures and meaning as the `CacheStore` methods of the same name (without `in_memory`, `from_env`, `is_shared`, `topology`):

| Method | Redis commands |
|---|---|
| `get(key)` | `GET`, then JSON-parse the text; invalid JSON is `CacheError::SerializationError` |
| `set(key, value, ttl_secs)` | `SET key <json> [EX ttl]` |
| `take(key)` | `GETDEL` (atomic), JSON-parse |
| `remove(key)` | `DEL` |
| `add_to_set(key, member, ttl_secs)` | `SADD`, then `EXPIRE key ttl` if `Some`; two commands, not a transaction |
| `set_members(key)` | `SMEMBERS` |
| `remove_from_set(key, member)` | `SREM` |
| `clear()` | `SCAN <cursor> MATCH <prefix>:* COUNT 500` loop, `DEL` one key at a time. On a cluster: `Redis::primaries()` (failure is `Backend("CLUSTER NODES failed: ..")`) and a sweep of each primary with `exec_on`. |

## `parse_primary`

```rust
#[cfg(feature = "redis")]
#[doc(hidden)]
pub fn parse_primary(line: &str) -> Option<(String, u16)>
```
`quench_cache::store::parse_primary`: a hidden forwarder to `quench_redis::parse_primary` kept for older callers.
