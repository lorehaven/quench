---
title: "API: client, pool, cluster"
parent: quench-redis
grand_parent: Crates
nav_order: 2
---

Reference for `Redis` (crate root), the `pool` module items (`Client`, `PoolOptions`, `PooledConnection`) and the `cluster` module items. All are re-exported at the crate root (`quench_redis::...`); the modules themselves (`pool`, `cluster`, `conn`, `cmd`, `config`, `error`) are private. Nothing in the crate is `unsafe`. Everything here is `Send + Sync` where its contents are (the types hold `Arc`s and `std::sync` locks); `Client` and `ClusterClient` are cheap to clone and shareable across tasks.

## Crate root: `Redis`

```rust
#[derive(Debug, Clone)]
pub enum Redis {
    Single(Client),
    Cluster(ClusterClient),
}
```

One server or a whole cluster behind one interface.

```rust
pub async fn connect(url: &str) -> Result<Self>
```
Connects to one server (`Client::connect`) and returns `Redis::Single`. Errors: see `Client::connect`.

```rust
pub async fn connect_cluster(urls: &[&str]) -> Result<Self>
```
Connects through seed nodes (`ClusterClient::connect`) and returns `Redis::Cluster`.

```rust
#[must_use]
pub const fn is_cluster(&self) -> bool
```
`true` for `Redis::Cluster`.

```rust
pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T>
```
Runs a command (routed by its key on a cluster) and converts the reply. Errors: transport errors, the server's error reply (`Error::Server`), `Error::Type` when the reply does not fit `T`.

```rust
pub async fn exec_on<T: FromValue>(&self, address: &str, command: &Cmd) -> Result<T>
```
Runs a command on the node `address` (`host:port`) of a cluster. On a single server the address is ignored and the command runs on the pool.

```rust
pub async fn primaries(&self) -> Result<Vec<String>>
```
The cluster's healthy primaries from `CLUSTER NODES`, as `host:port` (IPv6 hosts are bracketed). Empty `Vec` for a single server. Sends `CLUSTER NODES` to the first node of the current topology; `Error::Cluster("the topology is empty")` if there is none. Lines are filtered by `parse_primary`.

## Module `pool`

### `PoolOptions`

```rust
#[derive(Debug, Clone)]
pub struct PoolOptions {
    pub max_size: usize,          // default 8
    pub acquire_timeout: Duration, // default 10 s
    pub idle_timeout: Duration,    // default 300 s
}
impl Default for PoolOptions { .. }
```

| Field | Meaning | Default |
|---|---|---|
| `max_size` | Most connections open at once. The pool's semaphore is created with `max_size.max(1)` permits, so `0` behaves as `1`. | `8` |
| `acquire_timeout` | How long `Client::get` waits for a free connection. | 10 seconds |
| `idle_timeout` | An idle connection whose last use is older than this is dropped instead of reused. | 300 seconds |

### `Client`

```rust
#[derive(Clone)]
pub struct Client { /* Arc<Inner> */ }
```

A cheap-to-clone handle on a pool of connections to one server. `Debug` prints the address and idle count. A connection found dead when reused is replaced transparently; see the retry rule in [Internals](internals.md).

```rust
pub async fn connect(url: &str) -> Result<Self>
```
Parses `url` with `Config::from_url`, builds a pool with `PoolOptions::default()` and opens one connection now. Errors: `Error::Config` for the URL, otherwise whatever opening the first connection fails with (`Io`, `Tls`, `Timeout("connecting")`, `Server` such as `WRONGPASS`).

```rust
pub async fn connect_with(config: Config, options: PoolOptions) -> Result<Self>
```
As `connect`, with explicit configuration. The first connection is returned to the pool.

```rust
#[must_use]
pub fn lazy(config: Config, options: PoolOptions) -> Self
```
A client that connects on first use. Infallible.

```rust
#[must_use]
pub fn address(&self) -> String
```
The server's `host:port` (`Config::address`).

```rust
#[must_use]
pub fn idle_count(&self) -> usize
```
Number of idle connections currently in the pool.

```rust
pub async fn get(&self) -> Result<PooledConnection>
```
Checks out a connection: an idle one if `looks_dead()` is false and it is within `idle_timeout`, otherwise a new one (`Connection::connect`). Errors: `Error::Timeout("waiting for a free connection")` after `acquire_timeout`, `Error::Closed` if the semaphore was closed (never closed by this crate), or the connect error.

```rust
pub async fn exec_value(&self, command: &Cmd) -> Result<Value>
```
Runs a command and returns the reply, including a server error reply as `Value::Error` (the cluster layer reads redirects from it). Up to two attempts: if the first attempt used a reused connection and failed with `Error::Io`, `Error::Closed` or `Error::Protocol`, it is tried once more on another connection. Errors: transport and protocol errors only; after two failed attempts `Error::Closed`.

```rust
pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T>
```
`exec_value`, then `Value::Error(e)` becomes `Err(Error::Server(e))` and any other value goes through `T::from_value`.

```rust
pub async fn pipeline(&self, commands: &[Cmd]) -> Result<Vec<Value>>
```
Checks out a connection and calls `Connection::pipeline`. Error replies stay in the list as `Value::Error`. Not retried. An empty slice returns an empty `Vec` without a network round trip (the connection is still checked out).

```rust
pub async fn close(&self)
```
Closes every idle connection (shuts the streams down). Connections checked out are unaffected and return to the pool later.

### `PooledConnection`

```rust
pub struct PooledConnection { /* conn: Option<Connection>, client, permit */ }
impl Deref for PooledConnection { type Target = Connection; }
impl DerefMut for PooledConnection {}
impl Drop for PooledConnection {}
```

A connection checked out of a `Client`. All `Connection` methods are available through `Deref`. When dropped it returns to the pool unless `Connection::is_closed()` is true (closed, or a command is half-done), in which case it is discarded; the pool slot is released either way. `Debug` prints the inner connection.

## Module `cluster`

```rust
pub const SLOTS: u16 = 16384;
```
Number of hash slots.

```rust
#[must_use]
pub fn crc16(data: &[u8]) -> u16
```
CRC16/XMODEM (polynomial `0x1021`, initial value 0), the checksum Redis Cluster uses. `crc16(b"123456789") == 0x31c3`, `crc16(b"") == 0`.

```rust
#[must_use]
pub fn key_slot(key: &[u8]) -> u16
```
`crc16(hashed) % SLOTS`. `hashed` is the part of `key` between the first `{` and the next `}` after it, when that part is non-empty; otherwise the whole key. `key_slot(b"foo") == 12182`, `key_slot(b"somekey") == 11058`; `{user1000}.following` and `{user1000}.followers` share a slot; `foo{}{bar}` hashes the whole key; `foo{{bar}}zap` hashes `{bar`.

```rust
#[must_use]
pub fn parse_primary(line: &str) -> Option<(String, u16)>
```
Reads one `CLUSTER NODES` line (`<id> <host:port@cport[,hostname...]> <flags> ...`). Returns `(host, port)` when the flags (comma separated) contain `master` or `primary` and none starts with `fail` (`fail`, `fail?`); otherwise `None`. A host containing `:` is returned without brackets (the brackets are added by `Redis::primaries`).

### `SlotMap`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotMap { /* ranges: Vec<(u16, u16, String)> sorted by first slot */ }
```
Which node serves which slots.

```rust
pub fn from_cluster_slots(reply: &Value, default_host: &str) -> Result<Self>
```
Builds the map from a `CLUSTER SLOTS` reply (an array of `[first, last, [host, port, ...], replicas...]`). Only the first (primary) node of each entry is used. `default_host` stands in for a node whose host is missing or empty. Errors, all `Error::Cluster`: reply not a list (`CLUSTER SLOTS did not return a list`), entry not shaped right (`malformed CLUSTER SLOTS entry`), node without integer port (`a CLUSTER SLOTS node has no port`), slot values not fitting `u16` (`slot out of range`), `first > last` or `last >= SLOTS` (`slot range out of order`). Addresses are `host:port`, IPv6 bracketed. Ranges are sorted by first slot.

```rust
#[must_use] pub fn node_for(&self, slot: u16) -> Option<&str>
```
Address serving `slot` (binary search), or `None` if no range covers it.

```rust
#[must_use] pub fn addresses(&self) -> Vec<String>
```
Distinct node addresses in slot order.

```rust
pub fn assign(&mut self, slot: u16, address: &str)
```
Records that `slot` now lives on `address` (what a `MOVED` says), splitting the range that held it; adds a one-slot range if no range held it.

```rust
#[must_use] pub const fn is_empty(&self) -> bool
```

### `ClusterClient`

```rust
#[derive(Clone)]
pub struct ClusterClient { /* Arc<Inner> */ }
```
A client for a Redis Cluster. `Debug` prints the seed addresses. Holds one lazy `Client` pool per node address (created on first use, never evicted) and a `RwLock<SlotMap>`.

```rust
pub async fn connect(urls: &[&str]) -> Result<Self>
pub async fn connect_with(urls: &[&str], options: PoolOptions) -> Result<Self>
```
Connects through seed URLs (`redis://host:port`) and reads the topology with `refresh()`. Username, password, TLS flags and timeouts come from the **first** URL and apply to every node (`db` is forced to 0). Every URL contributes its address as a seed. `options` applies to each node's pool (`connect` uses `PoolOptions::default()`). Errors: `Error::Config("no cluster seed given")` for an empty list, `Error::Config` for a bad URL, `Error::Cluster("could not read the cluster topology (<last reason>)")` when no seed answers with a usable map.

```rust
pub async fn refresh(&self) -> Result<()>
```
Re-reads `CLUSTER SLOTS` from the nodes of the current map first, then the seeds, using the first one that answers with a non-empty valid map. Error: `Error::Cluster` as above.

```rust
#[must_use] pub fn nodes(&self) -> Vec<String>
```
Addresses of nodes currently serving slots (`SlotMap::addresses`).

```rust
pub async fn exec_value(&self, command: &Cmd) -> Result<Value>
```
Runs a command on the node owning `key_slot(command.key())`. A command without arguments after its name goes to the first node of the map. Follows redirects, at most 5 (`MAX_REDIRECTS`), so up to 6 attempts:

| Situation | Action |
|---|---|
| reply `-MOVED slot host:port` | update the slot map with `assign`, resend to the target |
| reply `-ASK slot host:port` | send `ASKING` and the command (pipelined) to the target; the map is not changed |
| `Io`, `Closed` or `Timeout` error, first time | `refresh()` once, re-route, retry |
| reply `-CLUSTERDOWN ...`, first time | `refresh()` once (its error is ignored), retry (to the same address as before) |
| any other server error reply | returned as `Ok(Value::Error)` |

A redirect target of the form `:port` (no host) means the node that sent it. After the redirect budget: `Error::Cluster("<NAME> was redirected more than 5 times")`. Empty topology: `Error::Cluster("the topology is empty")`.

```rust
pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T>
```
`exec_value` plus error-reply and type conversion, as `Client::exec`.

```rust
pub async fn exec_on<T: FromValue>(&self, address: &str, command: &Cmd) -> Result<T>
```
Runs on one specific node (creating its pool if needed), without routing or redirects. Errors: `Error::Cluster("bad node address ..")` for an address that is not `host:port`, transport errors, the server's error reply.
