---
title: Testing
parent: quench-cache
grand_parent: Crates
nav_order: 7
---

All test targets use the `quench-test` harness (`harness = false`).

| Target | Needs | Covers |
|---|---|---|
| `tests/unit.rs` (modules `data_cache_tests`, `request_cache_tests`, `store_tests`) | nothing | `DataCache` (set/get, TTL expiry, delete, len, clear, stats, cleanup, set semantics incl. expired and non-array replacement), `RequestCache` (keys, TTL eviction, `has` ignoring expiry, stats, cleanup, default TTL), in-memory `CacheStore` |
| `tests/from_env.rs` (feature `redis`) | nothing (one optional step) | `from_env`: in-memory by default, blank URL as unset, forced cluster mode failing against a non-cluster port, error texts. The environment is process-global, so everything is one test. Optionally connects when `CACHE_TEST_REDIS_URL` is set. |
| `tests/redis_store.rs` (feature `redis`) | `CACHE_TEST_REDIS_URL`; cluster tests need `CACHE_TEST_REDIS_CLUSTER_URL` (comma-separated seeds) | round trip and expiry, prefix-scoped `clear`, cluster round trip and sweep of every primary, atomic `take`, set membership and expiry, `clear` over many `SCAN` pages, invalid stored JSON as `SerializationError`, input validation of `connect`, `topology`/`is_shared` |

Redis tests return immediately when their variable is unset. Use a throwaway server: the tests write and `clear` under prefixes such as `forge-test`.

To test code that takes a `CacheStore`, use `CacheStore::in_memory()`; it needs no infrastructure and behaves like the Redis store except for the differences listed in [Internals](internals.md).
