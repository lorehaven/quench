---
title: Errors
parent: quench-pg
grand_parent: Crates
nav_order: 8
---

`quench_pg::{Error, Result, ServerError}` (`src/error.rs`).

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

## `enum Error`

```rust
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Config(String),
    Protocol(String),
    Auth(String),
    Tls(String),
    Server(Box<ServerError>),
    Timeout(&'static str),
    Closed,
    Decode(String),
    RowCount { expected: &'static str, got: usize },
}
```

| Variant | Display | When |
|---|---|---|
| `Io` | `i/o error: {err}` | transport failure; connect failures carry `connecting to {host}:{port}: ...` (or the socket path); a peer that closes the stream is `UnexpectedEof` |
| `Config` | `invalid configuration: {why}` | unusable URL or options; also more than 32767 parameters |
| `Protocol` | `protocol error: {why}` | invalid message framing or order; a data row before its description; a column value that is not UTF-8; unexpected message during startup; unexpected answer to `SSLRequest`. The connection is closed. |
| `Auth` | `authentication failed: {why}` | missing password, MD5 or unsupported method, bad SCRAM exchange |
| `Tls` | `TLS error: {why}` | handshake or certificate failure; server without TLS under `require`/`verify-full`; missing `tls` feature |
| `Server` | `{severity}: {message} (SQLSTATE {code})` plus `: {detail}` | the server reported an error; the connection remains usable (except during startup) |
| `Timeout` | `timed out {what}` | `"connecting"`, `"running the statement"`, `"waiting for a free connection"` |
| `Closed` | `the connection is closed or in an unknown state` | using a connection that is closed or was abandoned mid-statement |
| `Decode` | `could not convert a value: {why}` | `FromSql`/`Deserialize` failures, unknown column |
| `RowCount` | `expected {expected} row, got {got}` | `query_one` (`"exactly one"`), `query_opt` (`"at most one"`) |

Methods:

```rust
#[must_use] pub fn code(&self) -> Option<&str>               // SQLSTATE of a Server error
#[must_use] pub fn server(&self) -> Option<&ServerError>
#[must_use] pub fn is_unique_violation(&self) -> bool        // code == "23505"
```

`impl std::error::Error` (`source()` is the `io::Error` for `Io`, the `ServerError` for `Server`, `None` otherwise). `From<io::Error>` and `From<quench_serde::Error>` (to `Decode`) exist. `Error` is not `Clone` or `PartialEq`.

## `struct ServerError`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerError {
    pub severity: String,           // ERROR, FATAL, PANIC; or WARNING, NOTICE, DEBUG, INFO, LOG
    pub code: String,               // five-character SQLSTATE, e.g. "23505"
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub position: Option<u32>,      // 1-based character offset into the query
    pub schema: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
    pub constraint: Option<String>,
}
```

Parsed from the field list of `ErrorResponse`/`NoticeResponse` (code byte, NUL-terminated string, repeated, ended by a 0 byte). Field codes used: `V` or `S` severity (whichever of the two appears **first** in the message sets it; PostgreSQL sends the possibly localised `S` before `V`, so with a non-English `lc_messages` the severity text is the localised one; the unit test `malformed_field_lists_do_not_panic` pins `SFEHLER\0VERROR` to `FEHLER`), `C` code, `M` message, `D` detail, `H` hint, `P` position, `s` schema, `t` table, `c` column, `n` constraint; others (`F`, `L`, `R`, `W`, ...) are ignored. Malformed lists do not panic (missing terminators are tolerated). Display: `{severity}: {message} (SQLSTATE {code})`, plus `: {detail}` when present. `impl std::error::Error`.

## Behaviour after errors

* `Error::Server` during a query: the response is read to `ReadyForQuery`, so the connection is clean and reusable.
* `Error::Server` during startup (bad password, unknown database): the connection is closed.
* `Timeout("running the statement")`, a dropped query future and any I/O or protocol error leave the connection closed or busy: later calls return `Error::Closed`, and a pool discards it.
