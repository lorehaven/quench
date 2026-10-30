---
title: Configuration
parent: quench-cache
grand_parent: Crates
nav_order: 4
---

## Environment variables

Read once, by `CacheStore::from_env(key_prefix)`, through `quench_core::env::var` (unset or non-UTF-8 values count as unset).

| Variable | Type | Default | Meaning |
|---|---|---|---|
| `CACHE_URL` | URL list | unset | Redis URL. Checked first. Comma-separated URLs are cluster seed nodes (`redis://n1:7000,redis://n2:7000`). Blank (only whitespace) is unset. |
| `REDIS_URL` | URL list | unset | Used when `CACHE_URL` is not set. Same format. (A blank `CACHE_URL` does not fall through: the first variable that is *set* is taken, and if it is blank the result is in-process.) |
| `CACHE_CLUSTER` | `true` / other | unset | `true` (case-insensitive, trimmed) forces cluster mode even with one seed. Checked first. |
| `REDIS_CLUSTER` | `true` / other | unset | Used when `CACHE_CLUSTER` is not set. |

No URL means an in-process store. URL syntax and its query options (`connect_timeout`, `command_timeout`, `#insecure`) are those of [quench-redis](../quench-redis/configuration.md).

Programmatic configuration: `RedisStore::connect(url, prefix)` / `connect_with(url, prefix, cluster)`; the pool options of quench-redis are not exposed (defaults apply: 8 connections per node).

## Cargo features

See the [overview](index.md#features).

## Defaults of the in-process caches

* `DataCache`: no expiry unless `set_with_ttl` is used.
* `RequestCache::new()`: TTL 24 hours; `with_ttl(secs)` otherwise.
