---
title: API - pool
parent: quench-pg
grand_parent: Crates
nav_order: 3
---

`quench_pg::{Pool, PoolOptions, PooledConnection}` (`src/pool.rs`): at most `max_size` connections, reused across callers, with a health check on checkout and limits on age.

## `struct PoolOptions`

```rust
#[derive(Debug, Clone)]
pub struct PoolOptions {
    pub max_size: usize,            // default 5
    pub acquire_timeout: Duration,  // default 30 s: how long Pool::get waits for a free connection
    pub idle_timeout: Duration,     // default 10 minutes (600 s): idle connections older than this are closed instead of reused
    pub max_lifetime: Duration,     // default 30 minutes: connections older than this are closed when returned
    pub test_after_idle: Duration,  // default 30 s: a connection idle longer than this gets SELECT 1 before being handed out
}
```

`Default` gives exactly those values.

## `struct Pool`

A cheap-to-clone handle (`Clone`, an `Arc` inside). `Debug` shows `max_size` and the idle count. `Send + Sync` through its fields (a `Mutex<VecDeque<Connection>>` and a `tokio::sync::Semaphore`).

```rust
pub async fn connect(url: &str) -> Result<Self>
pub async fn connect_with(config: Config, options: PoolOptions) -> Result<Self>
#[must_use] pub fn lazy(config: Config, options: PoolOptions) -> Self
#[must_use] pub fn idle_count(&self) -> usize
#[must_use] pub fn available(&self) -> usize
pub async fn get(&self) -> Result<PooledConnection>
pub async fn close(&self)
```

* `connect`: `Config::from_url(url)` + default options + `connect_with`. Errors: `Error::Config` for a bad URL, or whatever opening the first connection fails with.
* `connect_with`: `lazy` then `get` once (the connection returns to the pool): a bad URL/password fails here.
* `lazy`: no connection now. The semaphore gets `max(options.max_size, 1)` permits.
* `idle_count`: connections sitting idle. `available`: permits left, i.e. how many more checkouts may succeed right now.
* `get`: waits for a permit up to `acquire_timeout` (`Error::Timeout("waiting for a free connection")`; a closed semaphore is `Error::Closed`). Then takes idle connections from the front of the queue: one that `looks_dead()` or is idle longer than `idle_timeout` is dropped and the next tried; one idle longer than `test_after_idle` is pinged and dropped if the ping fails. If none is usable, opens a new connection (`Connection::connect`; errors returned). The permit is held by the returned guard.
* `close`: closes every idle connection. Checked-out connections are not touched, and the pool stays usable afterwards.

Convenience methods; each does `self.get().await?` then the same-named `Connection` method on that connection and returns it to the pool:

```rust
pub async fn query(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>>
pub async fn query_one(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Row>
pub async fn query_opt(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>>
pub async fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<u64>
pub async fn batch_execute(&self, sql: &str) -> Result<()>
pub async fn simple_query(&self, sql: &str) -> Result<Vec<QueryResult>>
```

Each can additionally fail with `Pool::get`'s errors.

## `struct PooledConnection`

A connection checked out of a pool; returned on drop. `Debug` shows the inner connection.

```rust
pub async fn begin(&mut self) -> Result<Transaction<'_>>
impl Deref for PooledConnection { type Target = Connection; }
impl DerefMut for PooledConnection
```

All `Connection` methods are available through deref. On drop the connection goes back to the idle queue **unless** it `is_closed()` (closed, timed out, abandoned mid-statement), its `transaction_status()` is not `b'I'` (still inside, or in a failed, transaction), or it is older than `max_lifetime`; discarded connections are dropped (the TCP stream closes without a `Terminate`). A transaction guard dropped unfinished leaves status `T`, so such a connection is discarded rather than rolled back and reused.

Returning is synchronous (no I/O in `Drop`). The permit is released when the guard drops, in all cases.
