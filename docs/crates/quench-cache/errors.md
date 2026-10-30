---
title: Errors
parent: quench-cache
grand_parent: Crates
nav_order: 5
---

```rust
use quench_core::Error;

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Cache miss for key: {0}")]
    Miss(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Cache backend error: {0}")]
    Backend(String),
}
pub type Result<T> = std::result::Result<T, CacheError>;
```

| Variant | Produced by this crate when |
|---|---|
| `Miss(key)` | **Never.** Absent keys are `Ok(None)`. The variant exists for callers. |
| `SerializationError(msg)` | `RedisStore::get`/`take` read text from Redis that is not valid JSON for `quench_serde::Value`. |
| `Backend(msg)` | Any Redis failure (the `quench_redis::Error` text), connection failures (`Redis connection failed: ..`, `Redis cluster connection failed: ..`), `no Redis URL configured`, `CLUSTER NODES failed: ..`, and a configured URL without the `redis` feature. |

Writing to Redis cannot produce `SerializationError`: encoding a `Value` is infallible.

The in-process backend never returns an error.

`CacheError` converts into `quench_core::Error` through `?` (this is how `quench-auth` propagates it).
