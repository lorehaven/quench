# quench-redis
A Redis client: RESP2/RESP3, pooled connections with reconnect, pipelines and cluster routing (CRC16 slots, MOVED/ASK).

## What it is
`quench-redis` is Quench's Redis client. It replaces the `redis` crate with a RESP2/RESP3 codec, pooled connections that reconnect on demand, pipelines and Redis Cluster routing. It runs on `tokio` only; there is no blocking API.

`quench-cache` is its user in the workspace. Replies are converted through `FromValue`, and an error reply from the server becomes `Error::Server`.

## Use cases
- Run commands against one server or a cluster through `Redis`.
- Batch commands in one round trip with a pipeline.
- Hold a single connection for `SELECT`, `HELLO 3` or transactions.
- Back a cache layer with a pooled `Client`.

## Example
```rust
use quench_redis::{Client, Redis, Value, cmd};

let redis = Redis::connect("redis://localhost:6379").await?;
redis.exec::<()>(&cmd("SET").arg("k").arg("v").arg("EX").arg(&60)).await?;
let v: Option<String> = redis.exec(&cmd("GET").arg("k")).await?;

let client = Client::connect("redis://localhost:6379").await?;
let replies = client
    .pipeline(&[cmd("SET").arg("a").arg("1"), cmd("GET").arg("a")])
    .await?;
assert!(!matches!(replies[0], Value::Error(_)));
```

## Features
- `tls` (default): `rediss://` through `quench-tls`. Without it a TLS configuration fails to connect instead of falling back to plain text.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-redis/
