---
title: API - extractors, multipart, WebSocket, streams
parent: quench-http
grand_parent: Crates
nav_order: 4
---

Modules `extract`, `multipart`, `ws`, `stream`.

## `extract`

### `pub trait FromRequest: Sized`

```rust
#[async_trait]
pub trait FromRequest: Sized {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError>;
}
```

`#[get]` & co. call it for every handler parameter in declaration order. On `Err(e)` the handler is not called and the response is `IntoResponse::into_response(e)`. Implement it for your own types to add extractors.

### Extractors

All are tuple structs with a public field and `Deref` to the inner value (except `Inject`, which derefs to `T` through the `Arc`).

| Type | Definition | Reads | Bounds | Failure |
|---|---|---|---|---|
| `Path<T>` | `pub struct Path<T>(pub T)` | captured route parameters | `T: DeserializeOwned + Send` | `400` `invalid path parameter: ...` |
| `Query<T>` | `pub struct Query<T>(pub T)` | `uri.query()` (empty string if none), via `quench_serde::urlencoded::from_str` | `T: DeserializeOwned + Send` | `400` `invalid query string: ...` |
| `Json<T>` | `pub struct Json<T>(pub T)` | body, `quench_serde::json::from_slice`, capped by `BodyLimit` | `T: DeserializeOwned + Send` | `400` `invalid JSON body: ...` / body errors |
| `Form<T>` | `pub struct Form<T>(pub T)` | body, `quench_serde::urlencoded::from_bytes`, capped by `BodyLimit` | `T: DeserializeOwned + Send` | `400` `invalid form body: ...` |
| `Bytes` | `pub struct Bytes(pub quench_core::bytes::Bytes)` | raw body, capped by `BodyLimit` | - | `400` body errors |
| `Limited<T, const LIMIT: usize>` | `pub struct Limited<T, const LIMIT: usize>(pub T)` | as `Json<T>` / `Bytes` with a `LIMIT` byte cap instead of `BodyLimit` | implemented only for `Limited<Json<T>, LIMIT>` (`T: DeserializeOwned + Send`) and `Limited<Bytes, LIMIT>` | same as the wrapped one |
| `Extension<T>` | `pub struct Extension<T>(pub T)` | `req.extensions().get::<T>().cloned()` | `T: Clone + Send + Sync + 'static` | `500` ``` `<type_name>` was not set on this request``` |
| `Inject<T: ?Sized>` | `pub struct Inject<T: ?Sized>(pub Arc<T>)` | `req.container().get::<T>()` | `T: Any + Send + Sync + 'static`; `Clone` implemented | `500` with the `DiError::NotFound` text |

`Path<T>` shapes (`path_de::from_params`): captures are percent-decoded first (`%2f` becomes `/`; `+` stays `+`; a `%` not followed by two hex digits stays literal; invalid UTF-8 is an error `path parameter is not valid UTF-8`). Then `T` is tried against, in order: a map name-to-string (structs, maps), the single capture as a string (only if exactly one capture; `String`, numbers, `bool`), a positional array (tuples, `Vec`). Numbers and booleans are parsed from the text leniently. With no captures only shapes that accept an empty map/array succeed (`Vec<String>` gives `[]`; `String` fails). Errors that describe the data (`invalid integer`, ``missing field `digest` ``) are preferred over shape mismatch errors.

Body consumption: `Json`, `Form`, `Bytes`, `Limited`, `Multipart` call `take_body()`; a second consumer reads an empty body. No `Content-Type` header is checked.

### `pub const DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024`

Cap used when no `BodyLimit` was provided.

### `pub struct BodyLimit(pub usize)`

`#[derive(Debug, Clone, Copy)]`; `Default` is `BodyLimit(DEFAULT_BODY_LIMIT)`. Seed with `ContainerBuilder::provide(BodyLimit(n))`. Looked up per request with `container.get::<BodyLimit>()`; applies to `Json`, `Form`, `Bytes` and `Multipart`'s whole stream. `Limited` ignores it.

## `multipart`

Field-by-field `multipart/form-data` (no derive). Parsing is `quench_proto::multipart`.

### `pub struct Multipart` (`#[derive(Debug)]`)

```rust
pub async fn next_field(&mut self) -> Result<Option<Field<'_>>, HttpError>
```

Next field, `None` at the end; a field not read to its end is skipped. Errors are `HttpError::Extraction(ExtractError::InvalidMultipart(msg))` (`400`).

`FromRequest for Multipart`: reads the request `content-type` header (missing counts as empty), `parse_boundary` failure gives `400` ``invalid multipart body: missing or invalid multipart boundary``. Limits: `whole_stream = Some(BodyLimit)`, everything else `quench_proto::multipart::Limits::default()` (`per_field: None`, `max_header_bytes: 16 * 1024`, `max_fields: 1024`).

### `pub struct Field<'a>` (`#[derive(Debug)]`)

```rust
#[must_use] pub fn name(&self) -> Option<&str>
#[must_use] pub fn file_name(&self) -> Option<&str>
#[must_use] pub fn content_type(&self) -> Option<&str>
pub async fn chunk(&mut self) -> Result<Option<Bytes>, HttpError>   // next piece, None at end
pub async fn bytes(self) -> Result<Bytes, HttpError>
pub async fn text(self) -> Result<String, HttpError>                // UTF-8
```

`Bytes` here is `quench_core::bytes::Bytes`.

## `ws`

```rust
pub use quench_proto::ws::{CloseFrame, Config, Error, Message, WebSocket};
```

From `quench-proto`: `Config { max_message_size: usize, max_frame_size: usize }` (both default 16 MiB), `Message::{Text(String), Binary(Vec<u8>), Ping(Vec<u8>), Pong(Vec<u8>), Close(Option<CloseFrame>)}`, `CloseFrame { code: u16, reason: String }`, `WebSocket<Upgraded>` with `async send(&mut self, Message) -> Result<(), Error>`, `async recv(&mut self) -> Result<Option<Message>, Error>`, `async close(&mut self, code: u16, reason: &str) -> Result<(), Error>`. See `quench-proto` for details.

```rust
pub fn upgrade<F, Fut>(req: &Request, handler: F) -> Result<Response, HttpError>
pub fn upgrade_with<F, Fut>(req: &Request, config: Config, handler: F) -> Result<Response, HttpError>
// F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static, Fut: Future<Output = ()> + Send + 'static
```

Answer `101 Switching Protocols` and run `handler` once the connection switched. A request that is not a valid upgrade gives `HttpError::Status(400, <protocol error text>)`. `upgrade` is `upgrade_with` with `Config::default()`.

### `pub struct WebSocketUpgrade` (`#[derive(Debug, Clone)]`)

Extractor; validates the handshake via `quench_proto::ws::validate_upgrade(method, headers)` (`400` on failure). Private fields `key`, `config`.

```rust
#[must_use] pub const fn config(self, config: Config) -> Self
pub fn on_upgrade<F, Fut>(self, handler: F) -> Response   // same F/Fut bounds as above
```

## `stream`

Tokio sources as `quench_core::stream::Stream`s for `Response::streaming` / `Response::sse`.

```rust
#[must_use]
pub fn receiver_stream<T: Send + 'static>(receiver: mpsc::Receiver<T>)
    -> impl Stream<Item = T> + Send + Unpin + 'static
#[must_use]
pub fn reader_stream<R: AsyncRead + Unpin + Send + 'static>(reader: R)
    -> impl Stream<Item = std::io::Result<Bytes>> + Send + Unpin + 'static
```

`receiver_stream` yields every item until all senders are dropped. `reader_stream` yields 64 KiB chunks (the last may be shorter); a read error is yielded once and ends the stream. For other sources use `quench_core::stream::unfold`.
