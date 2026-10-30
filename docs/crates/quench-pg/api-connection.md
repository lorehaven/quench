---
title: API - Config and connections
parent: quench-pg
grand_parent: Crates
nav_order: 2
---

Crate root re-exports (`src/lib.rs`):

```rust
pub use config::{Config, SslMode};
pub use conn::{Connection, QueryResult, Stream, Transaction};
pub use error::{Error, Result, ServerError};
pub use pool::{Pool, PoolOptions, PooledConnection};
pub use row::{Row, RowIndex};
pub use types::{Column, FromSql, PgArray, ToSql};
pub mod message;   // public module
pub mod scram;     // public module
pub mod types;     // public module
```

`config`, `conn`, `error`, `pool` and `row` are private modules; their public items are reachable only through the re-exports. See [pool](api-pool.md), [rows](api-rows.md), [types](api-types.md), [protocol](api-protocol.md), [errors](errors.md).

## `enum SslMode`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SslMode {
    Disable,          // plain text only
    #[default] Prefer, // try TLS, fall back to plain text if the server declines; certificate not verified
    Require,          // TLS or fail; certificate not verified
    VerifyFull,       // TLS or fail; verify chain and server name
}
```

URL spellings: `disable`; `allow` and `prefer` (both `Prefer`); `require`; `verify-ca` and `verify-full` (both `VerifyFull`: the server name is always checked, so `verify-ca` is as strict). Any other text: `Error::Config("unknown sslmode `{text}`")`. The text is matched case-sensitively.

## `struct Config`

```rust
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,                     // hostname/IP, or Unix socket directory if it starts with '/'
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: Option<String>,
    pub ssl_mode: SslMode,
    pub ssl_root_cert_pem: Vec<Vec<u8>>,  // PEM certificate(s) trusted in addition to the public roots (VerifyFull)
    pub application_name: Option<String>,
    pub options: Option<String>,          // the `options` startup parameter, e.g. "-c search_path=app"
    pub connect_timeout: Duration,
    pub statement_timeout: Option<Duration>,
    pub max_message_size: usize,          // largest message accepted from the server
}
```

`impl Default`: `host "localhost"`, `port 5432`, `user "postgres"`, `password None`, `database None`, `ssl_mode Prefer`, `ssl_root_cert_pem []`, `application_name None`, `options None`, `connect_timeout 10 s`, `statement_timeout None`, `max_message_size 256 * 1024 * 1024`. `Debug` derives, so it prints the password: do not log a `Config`.

### `Config::from_url`

```rust
pub fn from_url(url: &str) -> Result<Self>
```

Grammar: `postgres://user:password@host:port/database?sslmode=require&application_name=x` (`postgresql://` too). Rules, all from the code:

* The scheme is required; otherwise `Error::Config("the URL must start with postgres:// or postgresql://")`.
* Everything is split as: query after the first `?`; path after the first `/` of the rest; userinfo before the **last** `@`.
* User, password, host, database and query values are percent-decoded (`%XX`); a bad escape or non-UTF-8 result is `Error::Config`. `+` is not turned into a space.
* Missing or empty parts keep the default: empty user keeps `postgres`; empty host keeps `localhost`; empty port (`host:`) keeps 5432; empty path means `database = None`.
* IPv6 hosts are written `[::1]` (`[::1]:6000` with port); an unterminated `[` is an error.
* A host given percent-encoded as a path (`%2Fvar%2Frun%2Fpostgresql`) becomes a Unix socket directory.
* The port must parse as `u16`, else `Error::Config("`{port}` is not a port number")`.
* The user part: the first `:` separates the password.
* Query parameters: `sslmode`; `sslrootcert` (a file path, read immediately with `std::fs::read`, repeated keys append; unreadable is `Error::Config("cannot read sslrootcert {path}: {error}")`); `application_name`; `options`; `connect_timeout` (whole seconds); `statement_timeout` (whole milliseconds). Non-numeric values for the last two are `Error::Config`. Unknown keys and keys without a value are ignored.
* `startup_params` (crate-private) sends: `user`; `database` (the `database` field, or the user name if `None`); `client_encoding=UTF8`; `TimeZone=UTC`; `DateStyle=ISO, MDY`; and, when set, `application_name` and `options`.

## `trait Stream`

```rust
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
```

A byte stream a connection runs over: TCP, a Unix socket, a TLS stream, or an in-memory pipe in tests.

## `struct QueryResult`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub rows: Vec<Row>,
    pub tag: String,       // command tag, e.g. "SELECT 3", "INSERT 0 1"; empty for an empty query
}
#[must_use] pub fn rows_affected(&self) -> u64    // message::rows_affected(&self.tag)
```

## `struct Connection`

One connection; every query method takes `&mut self`, so a connection serves one statement at a time. Not `Clone`. `Debug` shows `process_id`, `status` and `closed` only.

```rust
pub async fn connect(config: &Config) -> Result<Self>
```

Opens the transport (Unix socket when the host starts with `/` on Unix; otherwise TCP with `TCP_NODELAY`), negotiates TLS unless the transport is a Unix socket or `ssl_mode` is `Disable`, then runs `startup`. The whole sequence is bounded by `connect_timeout` (`Error::Timeout("connecting")`). TLS negotiation (feature `tls`): sends an `SSLRequest`; server answer `S` upgrades with `accept_invalid_certs = (mode != VerifyFull)`, `use_system_roots = (mode == VerifyFull)`, `extra_roots_pem = ssl_root_cert_pem`, no ALPN; the server name is `host`; answer `N` continues in plain text only for `Prefer` (otherwise `Error::Tls("the server does not support TLS")`); anything else is `Error::Protocol`. Without the feature see [index](index.md#features). Errors: any `Error`.

```rust
#[must_use] pub fn from_stream(stream: Box<dyn Stream>, config: Arc<Config>) -> Self
pub async fn startup(&mut self) -> Result<()>
```

`from_stream` wraps an already-open stream (tests use in-memory pipes); call `startup` next. `startup` sends the startup message and completes authentication: `AuthenticationOk` continues; cleartext password is answered with `Config::password` (`Error::Auth("the server asked for a password but none was configured")` if `None`); MD5 is refused (`Error::Auth("the server asked for MD5 authentication, which is not supported; use scram-sha-256")`); SASL requires the mechanism `SCRAM-SHA-256` among those offered (else `Error::Auth("the server offers only {mechanisms:?}; SCRAM-SHA-256 is required")`); any other method is `Error::Auth("unsupported authentication method {kind}")`; an `ErrorResponse` is `Error::Server` and closes the connection; any other message is `Error::Protocol("unexpected {msg:?} during startup")`. Finishes at the first `ReadyForQuery`.

State accessors:

```rust
#[must_use] pub const fn is_closed(&self) -> bool          // closed || busy (abandoned mid-statement)
#[must_use] pub const fn transaction_status(&self) -> u8    // b'I' idle, b'T' in transaction, b'E' failed transaction
#[must_use] pub const fn process_id(&self) -> u32           // backend pid
#[must_use] pub fn parameter(&self, name: &str) -> Option<&str>   // ParameterStatus values: server_version, TimeZone, ...
#[must_use] pub const fn created_at(&self) -> Instant
#[must_use] pub const fn last_used(&self) -> Instant        // last finished statement, or creation
pub fn looks_dead(&mut self) -> bool                        // closed, or a non-blocking read is ready (EOF or stray bytes)
```

Queries (all `async`; each first rolls back a pending dropped transaction, and fails with `Error::Closed` when the connection is closed or was abandoned mid-statement):

```rust
pub async fn simple_query(&mut self, sql: &str) -> Result<Vec<QueryResult>>
pub async fn batch_execute(&mut self, sql: &str) -> Result<()>
pub async fn query(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>>
pub async fn query_one(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Row>
pub async fn query_opt(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>>
pub async fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<u64>
pub async fn ping(&mut self) -> Result<()>
pub async fn begin(&mut self) -> Result<Transaction<'_>>
pub async fn close(mut self)
```

* `simple_query`: simple protocol, no parameters, `;`-separated statements; one `QueryResult` per statement (an empty query gives an empty-tag result). If a statement fails the server skips the rest and the first error is returned after the full response is read (rows already received are dropped).
* `batch_execute`: `simple_query` ignoring rows.
* `query`: extended protocol with an unnamed statement and portal (`Parse`, `Bind` with text parameters and text results, `Describe` portal, `Execute` with no row limit, `Sync`). `$1`, `$2`, ... are typed by the server. More than `i16::MAX` (32767) parameters: `Error::Config("{n} parameters; the protocol allows 32767")`. A server error is returned after the sync, and the connection stays usable.
* `query_one`: exactly one row, else `Error::RowCount { expected: "exactly one", got }`.
* `query_opt`: zero or one, else `Error::RowCount { expected: "at most one", got }`.
* `execute`: number of rows affected (parsed from the last word of the command tag; 0 if none).
* `ping`: `simple_query("SELECT 1")`.
* `begin`: `BEGIN`, then a guard. See below.
* `close`: sends `Terminate` and shuts the stream down (errors ignored; does nothing if already closed).

Statement timeout: `simple_query`, `batch_execute`, `query*` and `execute` run under `Config::statement_timeout` if set. On expiry the result is `Error::Timeout("running the statement")` and the connection is marked closed. Dropping a query future mid-flight also leaves the connection `is_closed()` (it is `busy`).

## `struct Transaction<'a>`

```rust
#[derive(Debug)]
pub struct Transaction<'a> { /* conn: &'a mut Connection, finished: bool */ }
```

Methods, each delegating to the connection with the same semantics: `query`, `query_one`, `query_opt`, `execute`, `batch_execute`, `simple_query`; and

```rust
pub async fn commit(mut self) -> Result<()>     // COMMIT
pub async fn rollback(mut self) -> Result<()>   // ROLLBACK
```

`commit` returns the server's error if the commit fails (the server rolls back). Dropping a `Transaction` that was neither committed nor rolled back sets a flag on the connection; the next statement on that connection first runs `ROLLBACK`. A failed statement inside the transaction leaves it in status `E`; subsequent statements fail until rollback (server behaviour).
