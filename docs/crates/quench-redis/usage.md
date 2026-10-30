---
title: Usage
parent: quench-redis
grand_parent: Crates
nav_order: 1
---

Worked examples from the common case to the advanced ones. All code uses `tokio`.

## 1. Run commands

Goal: set and read a key.

```rust
use quench_redis::{Redis, cmd};

let redis = Redis::connect("redis://localhost:6379").await?;
redis.exec::<()>(&cmd("SET").arg("k").arg("v").arg("EX").arg(&60)).await?;
let v: Option<String> = redis.exec(&cmd("GET").arg("k")).await?;
```

`exec::<T>` converts the reply with `FromValue`. `()` accepts any reply, `Option<T>` maps a nil reply to `None`. An error reply from the server becomes `Err(Error::Server(..))`; a reply that does not fit `T` is `Err(Error::Type(..))`.

Pitfalls: `Cmd::arg` takes a reference (`&T`), so numbers are passed as `&60`, `&ttl`. `"EX"` and similar keywords are ordinary string arguments.

## 2. Binary and numeric arguments, typed replies

```rust
redis.exec::<()>(&cmd("SET").arg("bin").arg(&b"\x00\xff"[..])).await?;
let raw: Vec<u8> = redis.exec(&cmd("GET").arg("bin")).await?;
let n: i64 = redis.exec(&cmd("INCRBY").arg("counter").arg(&5)).await?;
let members: Vec<String> = redis.exec(&cmd("SMEMBERS").arg("set")).await?;
let (cursor, keys): (u64, Vec<String>) =
    redis.exec(&cmd("SCAN").arg(&0).arg("MATCH").arg("app:*")).await?;
```

A `SCAN` reply is an array of a cursor (bulk string) and an array of keys: it reads as `(u64, Vec<String>)`. `Vec<u8>` reads a bulk string without UTF-8 checks; `String` fails with `Error::Type` on non-UTF-8 bytes.

## 3. Pipelines

Goal: several commands in one round trip, keeping per-command errors.

```rust
use quench_redis::{Client, Value, cmd};

let client = Client::connect("redis://localhost:6379").await?;
let replies = client
    .pipeline(&[cmd("SET").arg("a").arg("1"), cmd("LPUSH").arg("a").arg("x"), cmd("GET").arg("a")])
    .await?;
// replies[1] is Value::Error(ServerError { kind: "WRONGTYPE", .. }); the others are fine.
assert!(matches!(replies[1], Value::Error(_)));
```

`pipeline` returns `Vec<Value>` in command order; error replies stay in the list as `Value::Error`. Convert one with `FromValue::from_value`. `Redis` has no `pipeline`: use `Client` (single server). A cluster client has none either.

## 4. Hold one connection (transactions, `SELECT`, `HELLO 3`)

```rust
let mut conn = client.get().await?;      // PooledConnection, derefs to Connection
conn.request(&cmd("HELLO").arg(&3)).await?;   // switch this connection to RESP3
let reply = conn.request(&cmd("HGETALL").arg("h")).await?; // Value::Map under RESP3
```

The connection returns to the pool when the guard is dropped, unless it is closed or mid-command. The client speaks RESP2 by default and never sends `HELLO` itself; the decoder reads both versions.

## 5. Configure the pool and timeouts

```rust
use quench_redis::{Client, Config, PoolOptions};
use std::time::Duration;

let config = Config::from_url("redis://:secret@cache:6379/2?connect_timeout=3&command_timeout=1500")?;
let options = PoolOptions { max_size: 16, acquire_timeout: Duration::from_secs(2), ..PoolOptions::default() };
let client = Client::connect_with(config, options).await?;
```

`connect_timeout` is in seconds, `command_timeout` in milliseconds. A command that exceeds the command timeout fails with `Error::Timeout("waiting for the reply")` and its connection is discarded.

## 6. Lazy client

`Client::lazy(config, options)` does not connect; the first command does. Use it where startup must not depend on Redis being up.

## 7. Cluster

```rust
use quench_redis::{Redis, cmd};

let redis = Redis::connect_cluster(&["redis://node1:7000", "redis://node2:7000"]).await?;
redis.exec::<()>(&cmd("SET").arg("{user:1}:name").arg("Ann")).await?;   // routed by slot of the key
for node in redis.primaries().await? {                                  // "host:port" of healthy primaries
    let (_cursor, _keys): (u64, Vec<String>) =
        redis.exec_on(&node, &cmd("SCAN").arg(&0)).await?;
}
```

Routing uses the first argument after the command name as the key. Commands whose key is not the first argument (`EVAL`, `MGET` across slots, `XREAD`, ...) are not routed correctly: send them with `exec_on`. Hash tags (`{...}`) keep related keys on one node. Credentials and TLS settings come from the first seed URL and apply to all nodes; the `/db` of the URL is ignored.

## 8. Handle a specific server error

```rust
use quench_redis::Error;

match redis.exec::<i64>(&cmd("INCR").arg("not-a-number-key")).await {
    Err(Error::Server(e)) if e.kind == "WRONGTYPE" => { /* ... */ }
    Err(e) => return Err(e),
    Ok(n) => println!("{n}"),
}
```

## 9. Use the codec alone

```rust
use quench_redis::resp::{decode, encode_command, Value};

let mut out = Vec::new();
encode_command(&mut out, &[b"GET".to_vec(), b"k".to_vec()]);
assert_eq!(out, b"*2\r\n$3\r\nGET\r\n$1\r\nk\r\n");
assert_eq!(decode(b":42\r\n")?, Some((Value::Int(42), 5)));
assert_eq!(decode(b"$5\r\nhel")?, None);          // incomplete
```

## Pitfalls

* A reused pooled connection that fails with `Io`, `Closed` or `Protocol` is replaced and the command is sent again once. A command the server already executed can run twice: use idempotent commands across server restarts (details in [Internals](internals.md)).
* A connection that times out, or whose future is dropped mid-command, is discarded.
* `Cmd::key()` is simply the first argument; it is correct only for single-key commands.
