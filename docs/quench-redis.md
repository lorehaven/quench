# Quench Redis

`quench-redis` (crate `quench_redis`) is quench's Redis client. It replaces the `redis` crate: a RESP2/RESP3 codec, pooled connections that reconnect on demand, pipelines, and Redis Cluster routing. [Quench Cache](./quench-cache.md) is its user today.

```rust
let redis = quench_redis::Redis::connect("redis://localhost:6379").await?;
redis.exec::<()>(&cmd("SET").arg("greeting").arg("hello").arg("EX").arg(&60)).await?;
let greeting: Option<String> = redis.exec(&cmd("GET").arg("greeting")).await?;
```

## Features

`tls` (default) enables `rediss://` through [`quench-tls`](./quench-tls.md). Without it a configuration with TLS on fails to connect with `Error::Tls` ("built without its `tls` feature") rather than silently going plain.

## Commands and replies

`cmd("SET").arg("key").arg(&5)...` builds a command; arguments are anything `ToArg` (strings, byte slices, numbers). A reply is read as a Rust type through `FromValue`: `String`, `Vec<u8>`, `i64`, `u64`, `f64`, `bool`, `Option<T>` (nil), `Vec<T>`, `(A, B)` (a `SCAN` reply is `(u64, Vec<String>)`), `()` and the raw `Value`. A server error reply is `Error::Server` carrying its kind (`WRONGTYPE`, `NOAUTH`, `WRONGPASS`, `MOVED`, ...).

`resp` is the codec on its own: `Value` covers every RESP2 and RESP3 type (maps, sets, doubles, booleans, big numbers, verbatim strings, pushes, attributes), `decode` is incremental and bounds bulk sizes, element counts and nesting.

## Connections and the pool

`Config::from_url("redis://[user]:password@host:port/db?connect_timeout=5&command_timeout=2000")`; `rediss://` uses TLS (verified against the public and system roots; `#insecure` turns that off for development). A connection authenticates with `AUTH` and `SELECT`s the database on connect. `Client` is a cheap-to-clone pool (`max_size` 8, `acquire_timeout`, `idle_timeout`); a reused connection that turns out dead is replaced and the command retried once. That makes a command whose reply was lost run twice, so across a server restart prefer idempotent commands. `command_timeout` bounds a command; one that times out or is cancelled mid-flight leaves its connection unusable and the pool discards it. `client.pipeline(&[..])` sends several commands in one write and returns every reply, errors included.

The protocol is RESP2 unless the caller sends `HELLO 3` on a `Connection` itself.

## Cluster

`ClusterClient::connect(&["redis://node1:7000", "redis://node2:7000"])` reads `CLUSTER SLOTS`, then routes each command by the slot of its key (CRC16, honouring `{hash tags}`): the first argument after the command name, which is right for the single-key commands this client is used with. `MOVED` updates the routing table and retries; `ASK` sends `ASKING` and the command to the target without changing the table; a dead node or `CLUSTERDOWN` re-reads the topology once; more than five redirects is `Error::Cluster`. `exec_on(address, cmd)` runs a per-node command (`SCAN`), `Redis::primaries()` lists the healthy primaries from `CLUSTER NODES`.

`Redis` is the enum over both (`Redis::connect`, `Redis::connect_cluster`) with `exec`, `exec_on`, `primaries`.

## Testing

Unit tests for the codec (every type, partial input, limits, malformed input), URLs, conversions, CRC16 vectors, slot maps and redirect parsing. `tests/cluster.rs` runs the cluster client against scripted nodes that implement `CLUSTER SLOTS`/`NODES`, `GET`/`SET`/`SCAN`, `ASKING` and the redirect replies (routing, hash tags, `MOVED`, `ASK`, redirect loops, `CLUSTERDOWN`, a dead node). `tests/live.rs` runs against a real server when `CACHE_TEST_REDIS_URL` is set: strings, expiry, binary and megabyte values, sets, `SCAN`, error replies, pipelines, RESP3 replies, databases and `AUTH`, killed connections, timeouts and cancellation, pool limits.

[Home](../README.md)
