---
title: Multipart and blocking
parent: quench-client
grand_parent: Crates
nav_order: 3
---

## `multipart`

Source: `src/multipart.rs`. `multipart/form-data` bodies (RFC 7578) for uploads and mixed forms. `Bytes` is `quench_core::bytes::Bytes`.

### `Part`

```rust
#[derive(Debug, Clone)]
pub struct Part { /* bytes, file_name, mime */ }
```

| Method | Signature | Notes |
|---|---|---|
| `bytes` | `#[must_use] pub fn bytes(bytes: impl Into<Bytes>) -> Self` | raw bytes; no `Content-Type` unless a name or MIME is set |
| `text` | `#[must_use] pub fn text(text: impl Into<String>) -> Self` | a text field |
| `file_name` | `#[must_use] pub fn file_name(self, name: impl Into<String>) -> Self` | adds `; filename="..."`; if no MIME is set, `Content-Type: application/octet-stream` is written |
| `mime_str` | `#[must_use] pub fn mime_str(self, mime: impl Into<String>) -> Self` | the part's `Content-Type`, e.g. `image/png` (not validated) |

### `Form`

```rust
#[derive(Debug, Clone)]
pub struct Form { /* boundary, parts */ }
impl Default for Form   // = Form::new()
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | boundary `quench-boundary-{24 hex chars}` (12 random bytes) |
| `text` | `#[must_use] pub fn text(self, name: impl Into<String>, value: impl Into<String>) -> Self` | `part(name, Part::text(value))` |
| `part` | `#[must_use] pub fn part(self, name: impl Into<String>, part: Part) -> Self` | |
| `boundary` | `#[must_use] pub fn boundary(&self) -> &str` | |
| `content_type` | `#[must_use] pub fn content_type(&self) -> String` | `multipart/form-data; boundary={boundary}` |
| `into_body` | `#[must_use] pub fn into_body(self) -> Bytes` | the encoded body |

Body layout, per part in insertion order:

```text
--{boundary}\r\n
Content-Disposition: form-data; name="{name}"[; filename="{file}"]\r\n
[Content-Type: {mime}\r\n]
\r\n
{bytes}\r\n
```

and finally `--{boundary}--\r\n`. An empty form is only the closing line. In `name` and `filename`, `"` becomes `%22`, CR `%0D`, LF `%0A`. Bytes are written verbatim; the boundary is not checked against the data.

```rust
use quench_client::multipart::{Form, Part};
use quench_client::transport::Client;

# async fn run(client: Client, png: Vec<u8>) -> quench_client::transport::Result<()> {
let form = Form::new()
    .text("title", "hello")
    .part("upload", Part::bytes(png).file_name("a.png").mime_str("image/png"));
let response = client.post("http://localhost:8000/upload").multipart(form).send().await?;
# let _ = response; Ok(()) }
```

## `blocking` (feature `blocking`)

Source: `src/blocking.rs`. The same client for synchronous programs; each call blocks until the response is in. A `Client` owns a private **current-thread** tokio runtime (`Builder::new_current_thread().enable_all()`) shared by its clones, so the connection pool is reused between calls. Do not call it from inside an async runtime (tokio panics when a runtime is started or blocked on inside one).

It reuses `transport::{Error, HeaderMap, Method, Redirect, StatusCode}` and `multipart::Form`, and its `Result<T>` is `std::result::Result<T, transport::Error>` (private alias). Items:

### `blocking::Client`

```rust
#[derive(Clone, Debug)]
pub struct Client { /* transport::Client + Arc<Runtime> */ }
```

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` (panics if the runtime cannot start or the default configuration is invalid) |
| `builder` | `#[must_use] pub const fn builder() -> ClientBuilder` |
| `get`, `post`, `put`, `patch`, `delete`, `head` | `pub fn get(&self, url: impl AsRef<str>) -> RequestBuilder` |
| `request` | `pub fn request(&self, method: Method, url: impl AsRef<str>) -> RequestBuilder` |

`Default` = `Client::new()`.

### `blocking::ClientBuilder`

Methods (all `#[must_use]`, same meaning as in [Transport](api-transport.md#clientbuilder)): `timeout(Duration)`, `danger_accept_invalid_certs(bool)`, `add_root_certificate_pem(impl Into<Vec<u8>>)`, `redirect(Redirect)`, `client_identity_pem(impl Into<Vec<u8>>, impl Into<Vec<u8>>)`, `user_agent(impl Into<String>)`, and `pub fn build(self) -> Result<Client>` (`Error::Build("could not start the HTTP runtime: {e}")` if the runtime fails). **There is no `max_response_bytes`.**

### `blocking::RequestBuilder`

Methods (all `#[must_use]`, except `send`): `header`, `headers`, `basic_auth`, `bearer_auth`, `json`, `form`, `body`, `timeout`, `multipart(Form)`, `cookies`, `query` with the same signatures and behaviour as the transport builder, and

```rust
pub fn send(self) -> Result<Response>
```

There is **no `send_streaming`**.

### `blocking::Response`

`status()`, `headers()`, `cookies()`, `cookie(name)` (non-blocking accessors, `#[must_use]`), and the blocking `pub fn bytes(self) -> Result<Bytes>`, `pub fn text(self) -> Result<String>`, `pub fn json<T: DeserializeOwned>(self) -> Result<T>`. There is no `bytes_stream`.

```rust
use quench_client::blocking::Client;

fn main() -> Result<(), quench_client::transport::Error> {
    let client = Client::new();
    let body = client.get("http://localhost:8000/health").send()?.text()?;
    println!("{body}");
    Ok(())
}
```
