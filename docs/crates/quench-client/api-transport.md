---
title: Transport
parent: quench-client
grand_parent: Crates
nav_order: 4
---

Reference for `quench_client::transport` (source: `src/transport.rs`). This is the plain client the typed wrappers use. Use it directly for other methods, headers, raw bodies, streaming, cookies or multipart.

## Re-exports

```rust
pub use quench_proto::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
```

A caller needs no `quench-proto` dependency for these. `header` is the module of header-name constants.

## `Error` and `Result`

```rust
#[derive(Debug, quench_core::Error)]
pub enum Error {
    #[error("invalid request: {0}")]               Request(String),
    #[error("request failed: {0}")]                Transport(String),
    #[error("request timed out")]                  Timeout,
    #[error("could not decode response body: {0}")] Decode(String),
    #[error("could not build the client: {0}")]    Build(String),
}
pub type Result<T> = std::result::Result<T, Error>;
```

See [Errors](errors.md) for when each occurs.

## `Redirect`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redirect {
    /// Hand the 3xx response to the caller.
    None,
    /// Follow up to this many hops, then fail.
    Limited(usize),
}
```

## `Client`

```rust
#[derive(Debug, Clone)]
pub struct Client { /* Arc<Inner> */ }
```

Cheap to clone; clones share one connection pool, so create one `Client` and clone it. `Send + Sync`. `Default` = `Client::new()`.

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | `Client::builder().build()`; panics only if the default configuration were invalid (`expect("default client configuration is valid")`) |
| `builder` | `#[must_use] pub const fn builder() -> ClientBuilder` | defaults below |
| `get` / `post` / `put` / `patch` / `delete` / `head` | `pub fn get(&self, url: impl AsRef<str>) -> RequestBuilder` | `request(Method::X, url)` |
| `request` | `pub fn request(&self, method: Method, url: impl AsRef<str>) -> RequestBuilder` | parses the URL (a `#fragment` is dropped first); a parse failure is stored and returned by `send` as `Error::Request("{text}: {e}")`. A client-wide `User-Agent` is the only default header. |

## `ClientBuilder`

```rust
#[derive(Debug)]
pub struct ClientBuilder { /* private */ }
```

| Method | Signature | Default | Behaviour |
|---|---|---|---|
| `timeout` | `#[must_use] pub const fn timeout(self, timeout: Duration) -> Self` | none | default per-request timeout |
| `danger_accept_invalid_certs` | `#[must_use] pub const fn danger_accept_invalid_certs(self, accept: bool) -> Self` | `false` | skips certificate verification entirely |
| `add_root_certificate_pem` | `#[must_use] pub fn add_root_certificate_pem(self, pem: impl Into<Vec<u8>>) -> Self` | none | trusts an extra PEM CA on top of Mozilla and system roots; repeatable. A bad PEM makes `build` fail. |
| `client_identity_pem` | `#[must_use] pub fn client_identity_pem(self, certificate: impl Into<Vec<u8>>, private_key: impl Into<Vec<u8>>) -> Self` | none | client certificate chain and key (PEM) for mutual TLS; an unusable pair makes `build` fail |
| `user_agent` | `#[must_use] pub fn user_agent(self, value: impl Into<String>) -> Self` | none | sent on every request; a request's own `User-Agent` replaces it; an invalid header value makes `build` fail with ``Error::Build("`{agent}` is not a valid User-Agent")`` |
| `redirect` | `#[must_use] pub const fn redirect(self, policy: Redirect) -> Self` | `Redirect::Limited(10)` | |
| `max_response_bytes` | `#[must_use] pub const fn max_response_bytes(self, limit: usize) -> Self` | unlimited | a body larger than `limit` fails the request with `Error::Transport` |
| `build` | `pub fn build(self) -> Result<Client>` | | with `tls`: builds the rustls config (`ClientOptions { accept_invalid_certs, extra_roots_pem, use_system_roots: true, alpn_protocols: ["http/1.1"] }`); `Err(Error::Build(..))` for a bad root, identity or user agent |

Without the `tls` feature the TLS-related methods are accepted and ignored.

## `RequestBuilder`

```rust
pub struct RequestBuilder { /* client, method, url, headers, body, timeout, error */ }
```

`Debug` prints the method, URL, header **names only**, and timeout (so `Authorization` is never shown). The first error recorded by a builder method (invalid header, form encoding failure) is returned by `send` / `send_streaming`; later calls do not overwrite it.

| Method | Signature | Behaviour |
|---|---|---|
| `header` | `#[must_use] pub fn header(self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self` | **appends** (a name can occur several times) except `user-agent`, which replaces. An invalid name/value records ``Error::Request("invalid header `{name}`")``. |
| `headers` | `#[must_use] pub fn headers(self, headers: HeaderMap) -> Self` | appends every header of the map |
| `query` | `#[must_use] pub fn query<K: AsRef<str>, V: AsRef<str>>(self, pairs: &[(K, V)]) -> Self` | appends `k=v` pairs, percent-encoded, with `?` or `&` as needed; an empty slice changes nothing |
| `basic_auth` | `#[must_use] pub fn basic_auth(self, username: impl AsRef<str>, password: Option<&str>) -> Self` | `Authorization: Basic base64("user:pass")`; a `None` password is `""` |
| `bearer_auth` | `#[must_use] pub fn bearer_auth(self, token: impl AsRef<str>) -> Self` | `Authorization: Bearer {token}` |
| `timeout` | `#[must_use] pub const fn timeout(self, timeout: Duration) -> Self` | overrides the client timeout for this request |
| `json` | `#[must_use] pub fn json<T: Serialize + ?Sized>(self, body: &T) -> Self` | body = JSON; `Content-Type: application/json` |
| `form` | `#[must_use] pub fn form<T: Serialize + ?Sized>(self, body: &T) -> Self` | body = `application/x-www-form-urlencoded`; a serialisation error is recorded as `Error::Request` |
| `multipart` | `#[must_use] pub fn multipart(self, form: crate::multipart::Form) -> Self` | body = the encoded form; `Content-Type: multipart/form-data; boundary=...` |
| `cookies` | `#[must_use] pub fn cookies<K: AsRef<str>, V: AsRef<str>>(self, cookies: &[(K, V)]) -> Self` | one `Cookie: a=1; b=2` header; empty slice changes nothing. Names and values are not encoded. |
| `body` | `#[must_use] pub fn body(self, body: impl Into<Bytes>) -> Self` | raw body; set `Content-Type` yourself |
| `send` | `pub async fn send(self) -> Result<Response>` | sends and buffers the whole response body |
| `send_streaming` | `pub async fn send_streaming(self) -> Result<Response>` | returns when status and headers are in; the connection is not returned to the pool |

Order matters for headers set by body methods: `json`/`form`/`multipart` append `Content-Type`, so calling `.header("content-type", ..)` earlier gives two `Content-Type` headers. Call `body(..)` instead when you set your own.

The timeout wraps the whole exchange including redirects and, for `send`, the body download. For `send_streaming` it covers getting the headers only. An expired timeout is `Error::Timeout`.

## `Response`

```rust
#[derive(Debug)]
pub struct Response { /* status, headers, body (buffered or streaming), limit */ }
```

| Method | Signature | Behaviour |
|---|---|---|
| `status` | `#[must_use] pub const fn status(&self) -> StatusCode` | |
| `headers` | `#[must_use] pub const fn headers(&self) -> &HeaderMap` | |
| `cookies` | `#[must_use] pub fn cookies(&self) -> Vec<(String, String)>` | `(name, value)` for each `Set-Cookie` header; attributes (`Path`, `Expires`, ...) dropped, name/value trimmed; headers without `=` or not valid UTF-8 skipped |
| `cookie` | `#[must_use] pub fn cookie(&self, name: &str) -> Option<String>` | value of the first `Set-Cookie` with that name |
| `bytes` | `pub async fn bytes(self) -> Result<Bytes>` | the whole body; for a streaming response reads the rest, honouring `max_response_bytes` |
| `text` | `pub async fn text(self) -> Result<String>` | `bytes` decoded as UTF-8 **lossily** (invalid sequences become U+FFFD) |
| `json` | `pub async fn json<T: DeserializeOwned>(self) -> Result<T>` | `bytes` parsed as JSON; failure is `Error::Decode` |
| `bytes_stream` | `#[must_use] pub fn bytes_stream(self) -> BoxStream<'static, Result<Bytes>>` | chunks as they arrive (streaming response) or one chunk (buffered). Chunks are **not** limited by `max_response_bytes`. |

`Response` does not check the status: a 404 is an `Ok(Response)`. The typed wrappers add that check.

```rust
use quench_client::transport::{Client, Redirect};
use std::time::Duration;

# async fn run() -> quench_client::transport::Result<()> {
let client = Client::builder()
    .timeout(Duration::from_secs(10))
    .redirect(Redirect::None)
    .user_agent("my-tool/1.0")
    .build()?;

let response = client
    .post("https://api.example.org/items")
    .bearer_auth("token")
    .query(&[("dry_run", "true")])
    .json(&quench_serde::json!({"name": "a"}))
    .send()
    .await?;
if response.status() == quench_client::transport::StatusCode::FOUND {
    println!("{:?}", response.headers().get("location"));
}
# Ok(()) }
```

## Server-sent events and large downloads

```rust
use quench_core::stream::StreamExt;

# async fn run(client: quench_client::transport::Client) -> quench_client::transport::Result<()> {
let response = client.get("http://localhost:8000/events").send_streaming().await?;
let mut chunks = response.bytes_stream();
while let Some(chunk) = chunks.next().await {
    let chunk = chunk?;
    println!("{} bytes", chunk.len());
}
# Ok(()) }
```
