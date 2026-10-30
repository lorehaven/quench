---
title: API - request, response, errors
parent: quench-http
grand_parent: Crates
nav_order: 5
---

Modules `body`, `request`, `response`, `endpoint`, `error`.

## `body`

### `pub struct InboundBody(Body)`

An inbound request body, streamed (not pre-buffered). Wraps `quench_proto::body::Body`. `#[derive(Debug)]`. Implements `quench_core::stream::Stream<Item = Result<Bytes, BoxError>>` and `From<quench_proto::body::Body>`.

```rust
pub const fn empty() -> Self                                  // #[must_use]
pub fn from_bytes(bytes: impl Into<Bytes>) -> Self            // in-memory body, for tests
pub async fn collect_limited(self, limit: usize) -> Result<Bytes, ExtractError>
pub fn into_body(self) -> Body                                // #[must_use]; the protocol-level body
```

`collect_limited` buffers at most `limit` bytes. Errors: `ExtractError::BodyTooLarge(limit)` when more arrives, `ExtractError::BodyReadFailed(msg)` on a read error.

## `request`

### `pub struct Request`

`#[derive(Debug)]`. Private fields: method, uri, headers, captured path params, `prefix_skip`, `Option<InboundBody>`, `Arc<Container>`, `Extensions`.

```rust
pub fn new(method: Method, uri: Uri, headers: HeaderMap, body: InboundBody, container: Arc<Container>) -> Self // #[must_use]
pub const fn extensions(&self) -> &Extensions                 // #[must_use]
pub const fn extensions_mut(&mut self) -> &mut Extensions
pub const fn method(&self) -> &Method                         // #[must_use]
pub const fn uri(&self) -> &Uri                               // #[must_use]
pub const fn headers(&self) -> &HeaderMap                     // #[must_use]
pub fn header(&self, name: &str) -> Option<&str>              // #[must_use]; None if absent or not valid UTF-8
pub fn path_param(&self, name: &str) -> Option<&str>          // #[must_use]; first capture with that name, raw (not percent-decoded)
pub const fn container(&self) -> &Arc<Container>              // #[must_use]
pub fn take_body(&mut self) -> InboundBody                    // second call returns an empty body
pub const fn skip_prefix(&mut self, n: usize)                 // adds n bytes to the skipped path prefix (used by mount)
```

`uri()` always returns the original URI; `skip_prefix` only changes what the router matches (the pub(crate) `routed_path`). Extensions hold per-request data (use `Extension<T>` to read in handlers).

## `response`

### `pub struct Response`

`#[derive(Debug)]`; wraps `quench_proto::Response` (private field `inner`).

```rust
pub fn new(status: StatusCode) -> Self                        // #[must_use]; empty body, no headers
pub const fn status(&self) -> StatusCode                      // #[must_use]
pub const fn headers(&self) -> &quench_proto::HeaderMap       // #[must_use]
pub fn header(self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self        // #[must_use]; replaces
pub fn append_header(self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self // #[must_use]; keeps existing
pub fn cookie(self, cookie: &quench_proto::cookie::Cookie) -> Self                // #[must_use]; append_header("set-cookie", cookie.to_string())
pub fn text(status: StatusCode, body: impl Into<String>) -> Self   // content-type: text/plain; charset=utf-8
pub fn html(status: StatusCode, body: impl Into<String>) -> Self   // content-type: text/html; charset=utf-8
pub fn json<T: quench_serde::Serialize>(status: StatusCode, value: &T) -> Result<Self, quench_serde::Error> // content-type: application/json
pub fn from_bytes(status: StatusCode, bytes: Bytes) -> Self        // #[must_use]; no content-type
pub fn streaming<S, E>(status: StatusCode, stream: S) -> Self
    where S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static, E: Into<BoxError> + 'static
pub fn sse<S>(events: S) -> Self
    where S: Stream<Item = quench_proto::sse::Event> + Unpin + Send + Sync + 'static
pub fn ok() -> Self                                           // #[must_use]; 200, empty
pub fn not_found() -> Self                                    // #[must_use]; 404 text/plain "not found"
pub fn into_inner(self) -> quench_proto::Response             // #[must_use]
```

* `header`/`append_header` silently drop the header when the **value** is not a valid header value (`HeaderValue::from_str` fails) and, via the protocol layer's name conversion, when the name is invalid (no panic, no error).
* `json` never fails in practice (serialisation is infallible in `quench-serde`); the `Result` is kept for source compatibility.
* `streaming`: each `Ok` item is one chunk (chunked encoding); an `Err` ends the stream, the connection is closed after the headers were sent. The `Sync` bound exists because middleware holds `&Response` across awaits.
* `sse` delegates to `quench_proto::sse::response` (sets `content-type: text/event-stream`).

Conversions: `From<quench_proto::Response>`, `From<()>` (status `204`, empty), `From<String>` (`200 text/plain`), `From<&'static str>` (`200 text/plain`).

## `endpoint`

### `pub trait Endpoint: Send + Sync`

```rust
#[async_trait]
pub trait Endpoint: Send + Sync {
    async fn call(&self, req: Request) -> Response;
}
```

The unit of request handling. Implemented by `Router`, by the struct `#[get]` & co. generate, by `mount`'s wrapper and by `middleware::wrap`'s wrapper. Typical use is `Arc<dyn Endpoint>`.

### `pub trait IntoResponse`

```rust
pub trait IntoResponse { fn into_response(self) -> Response; }
```

Implemented for: `Response` (identity), `()` (`204`), `String` and `&'static str` (`200 text/plain`), `HttpError` (see below), `Json<T: Serialize>` (`200 application/json`; a serialisation error would become `500` with the error text), `Result<T, E>` where both implement `IntoResponse`. Handler return types must implement it. `ExtractError` and `DiError` do **not**; wrap them in `HttpError` (`?` does this for `ExtractError`).

## `error`

### `pub enum DiError` (`#[derive(Debug, quench_core::Error)]`)

```rust
MissingDependency { component: &'static str, dependency: &'static str }
Duplicate(&'static str)
Cycle(Vec<&'static str>)
ConstructionFailed { component: &'static str, reason: String }
NotFound(&'static str)
```

Messages (the `#[error(...)]` texts):

| Variant | Display |
|---|---|
| `MissingDependency` | ``component `{component}` depends on `{dependency}`, which is not registered (no #[injectable] or Container::provide for it)`` |
| `Duplicate` | ``component `{0}` is produced by more than one #[injectable]`` |
| `Cycle` | `dependency cycle detected among: ` + the names joined by ` -> ` |
| `ConstructionFailed` | ``constructing `{component}` failed: {reason}`` |
| `NotFound` | ``` `{0}` was requested from the container but is not registered``` |

`impl DiError { #[must_use] pub fn not_found<T: 'static>() -> Self }` builds `NotFound(std::any::type_name::<T>())`.

### `pub trait IntoHttpError: fmt::Debug + Send + Sync + 'static`

```rust
fn into_response(self: Box<Self>) -> Response;
```

Declared but implemented and used nowhere in the workspace (dead public API).

### `pub enum HttpError` (`#[derive(Debug, quench_core::Error)]`)

```rust
Status(quench_proto::StatusCode, String)         // #[error("{1}")]
Extraction(#[from] ExtractError)                 // #[error(transparent)]
```

```rust
pub fn status(code: quench_proto::StatusCode, msg: impl Into<String>) -> Self
#[must_use] pub fn into_response(self) -> Response
```

`into_response`: status is the code of `Status`, or `400 Bad Request` for every `Extraction` (including `BodyTooLarge`); the body is `Response::text(status, self.to_string())`. `IntoResponse for HttpError` calls it.

### `pub enum ExtractError` (`#[derive(Debug, quench_core::Error)]`)

| Variant | Display |
|---|---|
| `MissingPathParam(&'static str)` | ``missing path parameter `{0}` `` |
| `InvalidPath(String)` | `invalid path parameter: {0}` |
| `InvalidQuery(String)` | `invalid query string: {0}` |
| `InvalidJson(String)` | `invalid JSON body: {0}` |
| `InvalidForm(String)` | `invalid form body: {0}` |
| `InvalidMultipart(String)` | `invalid multipart body: {0}` |
| `BodyTooLarge(usize)` | `body exceeds the {0} byte limit` |
| `BodyReadFailed(String)` | `failed to read request body: {0}` |
| `MissingState(&'static str)` | ``` `{0}` is not available in application state``` |

`MissingPathParam` and `MissingState` are never constructed inside the crate.
