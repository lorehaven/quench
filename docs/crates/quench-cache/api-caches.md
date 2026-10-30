---
title: "API: DataCache and RequestCache"
parent: quench-cache
grand_parent: Crates
nav_order: 2
---

The two in-process caches. Both wrap an `Arc<quench_core::map::ShardedMap<String, _>>` and are internally synchronised: all methods take `&self`, and both types are `Send + Sync` when their contents are. Neither implements `Clone`.

## Module `data_cache` (feature `data-cache`)

### `CacheEntry`

```rust
#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub value: Value,
    pub timestamp: u64,          // Unix seconds when stored
    pub ttl_secs: Option<u64>,
}
#[must_use] pub fn is_expired(&self) -> bool
```
`is_expired` is true when `ttl_secs` is `Some(ttl)` and `now - timestamp >= ttl` (whole seconds, saturating). A TTL of `0` is therefore expired immediately.

### `CacheStats`

```rust
#[derive(Debug, Clone)]
pub struct CacheStats { pub total_entries: usize, pub expired_entries: usize, pub total_size_bytes: usize }
```
`total_size_bytes` is the sum of `value.to_string().len()` (JSON text length) over all entries.

### `DataCache`

```rust
#[derive(Debug)] pub struct DataCache { /* Arc<ShardedMap<String, CacheEntry>> */ }
impl Default for DataCache  // = new()
```

```rust
#[must_use] pub fn new() -> Self
pub fn set(&self, key: String, value: Value)
pub fn set_with_ttl(&self, key: String, value: Value, ttl_secs: u64)
#[must_use] pub fn get(&self, key: &str) -> Option<Value>
#[must_use] pub fn has(&self, key: &str) -> bool
#[must_use] pub fn delete(&self, key: &str) -> bool
```
`set` stores without expiry, `set_with_ttl` with a TTL in seconds; both overwrite and stamp the current time. `get` returns a clone of the value if it exists and is not expired; an expired entry is removed on the spot, unless a writer replaced it in the meantime. `has` is true for an existing, non-expired key and does not remove anything. `delete` returns whether an entry was removed (including an expired one).

```rust
pub fn add_to_set(&self, key: &str, member: &str, ttl_secs: Option<u64>)
#[must_use] pub fn set_members(&self, key: &str) -> Vec<String>
pub fn remove_from_set(&self, key: &str, member: &str)
```
A set is a `Value::Array` of `Value::String` members under the key. `add_to_set` updates in place under the map's entry lock, so concurrent adds do not lose members. It creates the set if absent, **replaces** an expired entry or one holding a non-array value with an empty set first, does not add a duplicate, and always sets the entry's timestamp to now and its `ttl_secs` to the argument (so `None` makes an existing expiring set permanent). `set_members` returns the string members (non-string array elements are skipped); empty for a missing, expired or non-array entry. `remove_from_set` removes the member and drops the key when the array becomes empty (only if it is still empty at that moment); no-op for a missing key or member.

```rust
pub fn clear(&self)
#[must_use] pub fn stats(&self) -> CacheStats
#[must_use] pub fn cleanup_expired(&self) -> usize
#[must_use] pub fn len(&self) -> usize
#[must_use] pub fn is_empty(&self) -> bool
```
`clear` removes everything. `stats` walks all entries. `cleanup_expired` removes expired entries and returns how many. `len` counts entries including expired ones not yet removed.

## Module `request_cache` (feature `request-cache`)

### `CachedResponse`

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResponse {
    pub content: String,
    pub model: String,
    pub timestamp: u64,           // Unix seconds; the caller sets it
    pub tokens_used: Option<u32>,
}
```

### `CacheStats`

A second, separate type with the same shape as the `data_cache` one (`total_entries`, `expired_entries`, `total_size_bytes`; `#[derive(Debug, Clone)]`). Here `total_size_bytes` is the sum of `content.len()`.

### `RequestCache`

```rust
#[derive(Debug)] pub struct RequestCache { /* cache, ttl_secs */ }
impl Default for RequestCache  // = new()
```
One TTL for the whole cache; entries do not carry their own.

```rust
#[must_use] pub fn new() -> Self                // TTL 24 hours (86 400 s)
#[must_use] pub fn with_ttl(ttl_secs: u64) -> Self
#[must_use] pub fn generate_key(parts: &[&str]) -> String
```
`generate_key` joins the parts with `-`, hashes the string with `std::collections::hash_map::DefaultHasher::new()` and returns `cache_<u64 hash in decimal>`. The hash is deterministic within one build of Rust but `DefaultHasher`'s algorithm is not guaranteed stable across Rust releases, and `["a-b","c"]` and `["a","b-c"]` give the same key.

```rust
#[must_use] pub fn get(&self, key: &str) -> Option<CachedResponse>
pub fn set(&self, key: String, response: CachedResponse)
#[must_use] pub fn has(&self, key: &str) -> bool
pub fn clear(&self)
#[must_use] pub fn stats(&self) -> CacheStats
#[must_use] pub fn cleanup_expired(&self) -> usize
```
Expiry uses the **entry's own `timestamp`**: an entry is valid while `now - timestamp < ttl_secs`. `get` returns a clone when valid; otherwise it removes the entry (if it is still expired) and returns `None`. `set` stores the response as given (it does not stamp the time). `has` is true when the key is present **even if expired**. `stats` counts entries with `now - timestamp >= ttl` as expired. `cleanup_expired` removes them and returns the count. There is no `len`.
