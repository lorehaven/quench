---
title: Internals
parent: quench-cache
grand_parent: Crates
nav_order: 6
---

## Key layout on Redis

`<prefix>:<key>` for every key; `clear` matches `<prefix>:*`. The prefix is used verbatim in a glob pattern (characters such as `*`, `?`, `[` in a prefix are not escaped). With an empty prefix keys are stored unprefixed and `clear` uses the pattern `*`, deleting the whole database.

Values are the JSON text of `quench_serde::Value` (`quench_serde::json::to_string`). Sets are native Redis sets of plain strings.

## Atomicity

| Operation | Redis | In-process |
|---|---|---|
| `take` | `GETDEL`: atomic | `get` then `delete`: two steps; two threads can both observe the value |
| `add_to_set` | `SADD` then `EXPIRE`: two commands, no transaction; a failure between them leaves a set without expiry | one in-place update under the map's entry lock |
| `remove_from_set` | `SREM`; empty sets vanish | in-place update, then a conditional remove if still empty |
| `clear` | not atomic: a `SCAN` loop with one `DEL` per key | atomic clear |

## TTL behaviour

* Redis `SET .. EX n` and `EXPIRE key n`; `n = 0` is an error from Redis. On Redis `add_to_set(.., None)` leaves an existing expiry unchanged.
* In-process expiry is evaluated lazily on `get` / `has` / `set_members`, in whole seconds from `SystemTime`; a wall-clock change affects it. `add_to_set` overwrites the TTL with its argument.

## Cluster behaviour

`clear` on a cluster asks `Redis::primaries()` (which runs `CLUSTER NODES` and keeps healthy masters) and runs `SCAN` on each primary separately, because a `SCAN` cursor is only meaningful to the node that issued it. `DEL` is issued per key so that no cross-slot multi-key command is ever sent. All other commands are single-key and are routed by quench-redis.

## `RequestCache` details

Entries keep their own `timestamp` (set by the caller); the cache TTL is global. `has` ignores expiry, `get` honours it. See [API](api-caches.md#requestcache).

## Concurrency

`ShardedMap` provides per-key atomic update (`update`, `update_existing`, `remove_if`, `with`), which `DataCache` uses for sets and for the get-or-expire race (a concurrent writer's fresh value is not removed by a reader that saw the old, expired one). No `unsafe` code in this crate.
