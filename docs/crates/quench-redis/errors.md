---
title: Errors
parent: quench-redis
grand_parent: Crates
nav_order: 6
---

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

## `Error`

```rust
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Config(String),
    Protocol(String),
    Server(ServerError),
    Tls(String),
    Timeout(&'static str),
    Type(String),
    Cluster(String),
    Closed,
}
```

Implements `Display`, `std::error::Error` (`source()` returns the inner error for `Io` and `Server`, `None` otherwise) and `From<io::Error>` (becomes `Io`). `Error` is not `Clone` or `PartialEq`.

| Variant | `Display` | When |
|---|---|---|
| `Io` | `i/o error: {err}` | Socket errors, including `UnexpectedEof` when the peer closes mid-reply, and `connecting to host:port: ..` on connect failure. The connection is marked closed. |
| `Config` | `invalid configuration: {why}` | `Config::from_url` failures, empty cluster seed list. |
| `Protocol` | `protocol error: {why}` | Bytes that are not RESP, or over the size limits. The connection is marked closed. |
| `Server` | `server error: {err}` | The server replied `-KIND message` to a command executed through `request`, `exec` etc. |
| `Tls` | `TLS error: {why}` | TLS configuration, server-name or handshake failure; the `tls` feature missing. |
| `Timeout` | `timed out {what}` | `"connecting"` (connect timeout), `"waiting for the reply"` (command timeout; connection discarded), `"waiting for a free connection"` (pool acquire timeout). |
| `Type` | `unexpected reply: {why}` | `FromValue` could not convert the reply. |
| `Cluster` | `cluster error: {why}` | Unusable topology, `CLUSTER SLOTS` shape errors, bad node addresses, more than 5 redirects. |
| `Closed` | `the connection is closed` | The connection is closed or busy; a pooled retry that failed twice. |

## `ServerError`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerError {
    pub kind: String,      // first word: ERR, WRONGTYPE, MOVED, ASK, NOAUTH, WRONGPASS, CLUSTERDOWN, ...
    pub message: String,   // the rest
}
impl ServerError { #[must_use] pub fn parse(text: &str) -> Self }
```

`parse` splits at the first space: `parse("ERR")` gives kind `ERR` and an empty message. `Display` is `"{kind} {message}"` (with a trailing space when the message is empty). Implements `std::error::Error`.

Where it appears: `Error::Server(ServerError)` from `exec`/`request`; `Value::Error(ServerError)` from `exec_value`, `request_value`, `pipeline`, and for errors inside aggregates.

`MOVED` and `ASK` replies are consumed by `ClusterClient`; `WRONGPASS`/`NOAUTH` come from `AUTH` during connect.
