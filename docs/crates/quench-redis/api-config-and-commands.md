---
title: "API: configuration, commands, connection"
parent: quench-redis
grand_parent: Crates
nav_order: 3
---

Reference for `Config`, the command builder and reply conversion (`Cmd`, `cmd`, `ToArg`, `FromValue`) and the single connection (`Connection`, `Stream`). All re-exported at the crate root. Errors are in [errors.md](errors.md).

## `Config`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub db: u32,
    pub tls: bool,
    pub tls_insecure: bool,
    pub connect_timeout: Duration,
    pub command_timeout: Option<Duration>,
}
```

| Field | Meaning | `Default` |
|---|---|---|
| `host` | Server host name or IP (no brackets for IPv6). | `"127.0.0.1"` |
| `port` | TCP port. | `6379` |
| `username` | ACL user; used only together with a password. | `None` |
| `password` | Sent with `AUTH` on connect. | `None` |
| `db` | Logical database; `SELECT n` is sent when it is not 0. Ignored on a cluster. | `0` |
| `tls` | `rediss://`: TLS, verifying the certificate against the public and system roots. | `false` |
| `tls_insecure` | Accept any certificate (development only). | `false` |
| `connect_timeout` | Limit for TCP connect, TLS handshake, `AUTH` and `SELECT` together. | 5 s |
| `command_timeout` | Limit for one command and its reply (a whole pipeline counts as one); `None` disables. | `Some(30 s)` |

`impl Default for Config` gives the values above.

```rust
pub fn from_url(url: &str) -> Result<Self>
```
Parses `redis://[[user]:password@]host[:port][/db][?connect_timeout=secs&command_timeout=millis]`, or `rediss://` for TLS. Rules, all checked against the code:

* The scheme must be `redis://` or `rediss://`; otherwise `Error::Config("the URL must start with redis:// or rediss://")`.
* A fragment `#insecure` on a `rediss://` URL sets `tls_insecure`; it is ignored for `redis://`.
* Userinfo is split at the last `@`; user and password at the first `:`. An empty user means `None`. User, password and host are percent-decoded (`%40` is `@`); a bad escape or non-UTF-8 result is `Error::Config`. `+` is not treated as a space.
* Host may be bracketed IPv6 (`[::1]:7000`); an empty host keeps the default `127.0.0.1`. A missing or empty port keeps `6379`. A port that is not a `u16` is `Error::Config("`x` is not a port number")`.
* The path after the first `/` is the database number (`u32`); an empty path keeps `0`; anything else is `Error::Config("`x` is not a database number")`.
* Query pairs: `connect_timeout` in whole **seconds**, `command_timeout` in **milliseconds** (the latter becomes `Some(..)`). Unknown keys and pairs without `=` are ignored. A non-numeric value is `Error::Config`.
* A username without a password (`redis://user@host`) parses, but no `AUTH` is sent, because `AUTH` is only issued when a password is set.

```rust
#[must_use] pub fn address(&self) -> String
```
`host:port`, with the host in brackets when it contains `:`.

## Commands

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cmd { /* args: Vec<Vec<u8>> */ }

#[must_use]
pub fn cmd(name: &str) -> Cmd
```
`cmd("SET")` starts a command whose first argument is the name (sent as given; `name()` uppercases it for display).

### `Cmd` methods

```rust
#[must_use] pub fn arg<T: ToArg + ?Sized>(mut self, value: &T) -> Self
#[must_use] pub fn args<T: ToArg>(mut self, values: &[T]) -> Self
#[must_use] pub fn name(&self) -> String
#[must_use] pub fn get(&self, index: usize) -> Option<&[u8]>
#[must_use] pub fn key(&self) -> Option<&[u8]>
```

* `arg` appends one argument. It takes a reference: `.arg("key")`, `.arg(&5)`, `.arg(&bytes[..])`.
* `args` appends every element of the slice.
* `name` is the command name uppercased (lossy UTF-8); empty string for an empty `Cmd` (`Cmd::default()`).
* `get(0)` is the name, `get(1)` the first argument.
* `key` is `get(1)`: the key a cluster routes by. Correct for single-key commands only; keyless commands (`SCAN`, `CLUSTER`, `PING`) have none that matters and are routed by the caller.

The wire encoding of a `Cmd` is the multi-bulk format (`*N\r\n$len\r\narg\r\n...`); the encoder is `pub(crate)`, the public one is `resp::encode_command`.

### `ToArg`

```rust
pub trait ToArg { fn to_arg(&self) -> Vec<u8>; }
```

Implemented for: `str`, `String`, `[u8]`, `Vec<u8>` (bytes as-is), `i8 i16 i32 i64 isize u8 u16 u32 u64 usize f32 f64` (decimal text from `to_string()`), and `&T` for any `T: ToArg + ?Sized`. `bool` has no implementation: pass `"1"`/`"0"`.

### `FromValue`

```rust
pub trait FromValue: Sized { fn from_value(value: Value) -> Result<Self>; }
```
Reads a reply as a Rust type; failure is `Error::Type("expected <what>, got <Value debug>")` (or a specific message below). Implementations:

| Type | Accepts |
|---|---|
| `Value` | anything, unchanged |
| `()` | anything, discarded |
| `String` | `Bulk` (must be UTF-8, else `Error::Type("a string that is not UTF-8")`), `Simple`, `Verbatim`, `BigNumber`, `Int` (as decimal text) |
| `Vec<u8>` | `Bulk`, `Simple`, `Verbatim` |
| `i64` | `Int`; `Bulk`/`Simple` parsed as an integer (`Error::Type("`t` is not an integer")`) |
| `u64` | as `i64`, then negative is `Error::Type("-1 is negative")` |
| `f64` | `Double`, `Int`, `Bulk`/`Simple` parsed as a number |
| `bool` | `Bool`; `Int` (non-zero is `true`); `Simple("OK")` is `true` |
| `Option<T>` | `Nil` is `None`, anything else `Some(T::from_value(..)?)` |
| `Vec<T>` | `Array` or `Set` element-wise; `Nil` is an empty vector. (`Vec<u8>` is the byte-string impl above.) |
| `(A, B)` | an `Array` of exactly two elements |

`Map`, `Push`, `Double` for strings and `Error` elements inside arrays are not convertible except through `Value`.

## `Connection` and `Stream`

```rust
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
```
Any byte stream a connection can run over (TCP, TLS, in-memory pipes in tests).

```rust
pub struct Connection { /* boxed stream, read buffer, state flags, timestamps */ }
```
One connection to one server; commands are strictly request/reply (no multiplexing). `Debug` shows `closed` and `busy`. Methods taking `&mut self` need exclusive access.

```rust
pub async fn connect(config: &Config) -> Result<Self>
```
Opens TCP (`TCP_NODELAY` set, best effort), negotiates TLS when `config.tls`, then `handshake`. The whole sequence is limited by `connect_timeout` (`Error::Timeout("connecting")`). A TCP failure is `Error::Io` with the message `connecting to host:port: <cause>`. TLS failures are `Error::Tls`; without the `tls` feature and `config.tls == true`: `Error::Tls("quench-redis was built without its `tls` feature")`. A wrong password is `Error::Server` with kind `WRONGPASS`.

```rust
#[must_use] pub fn from_stream(stream: Box<dyn Stream>, config: &Config) -> Self
```
Wraps an open stream without authenticating (tests use in-memory pipes). Only `config.command_timeout` is taken.

```rust
pub async fn handshake(&mut self, config: &Config) -> Result<()>
```
`AUTH password` or `AUTH username password` when a password is set, then `SELECT db` when `db != 0`. Errors: the server's refusal as `Error::Server`, or transport errors.

```rust
#[must_use] pub const fn is_closed(&self) -> bool
```
`true` when the connection can no longer be used: closed, or a command was sent and its reply not completely read (`busy`).

```rust
#[must_use] pub const fn created_at(&self) -> Instant
#[must_use] pub const fn last_used(&self) -> Instant
```
Creation time; time of the last completed exchange (initially the creation time).

```rust
pub fn looks_dead(&mut self) -> bool
```
Polls the stream once without blocking: `true` if the connection is closed/busy, or the stream is readable (the peer closed it, or sent stray bytes). `false` only when the read is pending.

```rust
pub async fn request_value(&mut self, command: &Cmd) -> Result<Value>
```
Sends one command, returns its reply; a server error reply is returned as `Ok(Value::Error)`. Errors: transport and protocol errors, `Error::Closed` if the connection is closed or busy, `Error::Timeout("waiting for the reply")` after `command_timeout` (the connection is then closed).

```rust
pub async fn request(&mut self, command: &Cmd) -> Result<Value>
```
As `request_value`, but an error reply is `Err(Error::Server(..))`.

```rust
pub async fn pipeline(&mut self, commands: &[Cmd]) -> Result<Vec<Value>>
```
Writes all commands in one write and reads that many replies; error replies stay in the list. Empty slice returns an empty `Vec`. The command timeout covers the whole pipeline.

```rust
pub async fn ping(&mut self) -> Result<()>
```
`PING`; errors as `request`.

```rust
pub async fn close(mut self)
```
Shuts the stream down (errors ignored).

Cancel safety: dropping a future of `request*`/`pipeline` while it is running leaves the connection `busy`, so `is_closed()` is `true` and a pooled connection is discarded.
