---
title: API net
parent: quench-ui-web
grand_parent: Crates
nav_order: 5
---

`fetch` and server-sent events as futures. Both are thread-local, `!Send`, and need the executor ([`spawn`](api-core.md#executor-and-timers-srcexecrs)) to be awaited from.

## Fetch (`src/fetch.rs`)

### `Request`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request { /* method, url, headers, body (private) */ }

impl Request {
    #[must_use] pub fn new(method: &str, url: &str) -> Self;
    #[must_use] pub fn get(url: &str) -> Self;
    #[must_use] pub fn post(url: &str) -> Self;
    #[must_use] pub fn header(self, name: &str, value: &str) -> Self;
    #[must_use] pub fn body(self, body: impl Into<Vec<u8>>) -> Self;
    #[must_use] pub fn send(self) -> Fetch;
}
```

* `new` upper-cases the method (`"post"` becomes `POST`). Any method string is accepted; the browser decides. `get` and `post` call `new`.
* `header` appends a header. It is **silently dropped** when the name or value contains `\n` or `\r`, or when the name contains `:` (headers cross the boundary as `name: value` lines). Headers are not de-duplicated.
* `body` sets the body bytes (`String`, `&str`, `Vec<u8>`, `&[u8]` all work through `Into<Vec<u8>>`). An empty body sends none. For `GET` and `HEAD` the body is never sent (the JS side skips it).
* `send` starts the request **now** (it calls `q_fetch` immediately) and returns the future. Ids are a wrapping `u32` counter.

On the JS side the request is `fetch(url, { method, headers, body, signal })`; credentials and mode are the browser defaults. A URL that starts with `quench-bridge:` is not a network request (see [API: bridge](api-bridge.md)).

### `Response`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    #[must_use] pub const fn ok(&self) -> bool;
    #[must_use] pub fn text(&self) -> String;
    #[must_use] pub fn header(&self, name: &str) -> Option<&str>;
}
```

* `ok()`: `200 <= status < 300`.
* `text()`: the body as UTF-8, invalid bytes replaced by U+FFFD.
* `header(name)`: the first header whose name matches ASCII case-insensitively.
* `headers`: parsed from `name: value` lines, split at the first `:`, both trimmed. Lines without `:` are skipped. The browser's header names are lower case. Duplicate headers arrive combined as the browser's `Headers` iteration gives them.

A response with an HTTP error status (404, 500) is an `Ok(Response)`, not a `FetchError`.

### `FetchError`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    Network(String),
    Aborted,
}
impl fmt::Display for FetchError   // "network error: {message}" / "request aborted"
impl std::error::Error for FetchError
```

`Network` carries the browser's message (offline, refused, blocked by CORS). `Aborted` appears when the request was aborted through `AbortHandle` and the browser then reported status 0.

### `Fetch`

```rust
#[derive(Debug)]
pub struct Fetch { /* id, done */ }
impl Fetch {
    #[must_use]
    pub const fn abort_handle(&self) -> AbortHandle;
}
impl Future for Fetch { type Output = Result<Response, FetchError>; }
```

Dropping a `Fetch` that has not completed forgets the request and tells the host to abort it; a late completion is ignored. Awaiting it after it resolved is not possible (it is consumed).

### `AbortHandle`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbortHandle { /* id */ }
impl AbortHandle {
    pub fn abort(&self);
}
```

`abort()` marks the request aborted and calls `q_fetch_abort`; the browser then completes it with status 0 and the future resolves to `Err(FetchError::Aborted)`. It does nothing if the request already has a result, was already aborted, or its `Fetch` was dropped. `AbortHandle` is `Copy`.

### `pending_fetches`

```rust
#[must_use]
pub fn pending_fetches() -> usize
```

Requests started and not yet resolved (and not dropped).

### Example

```rust
use quench_ui_web::{Request, spawn};

spawn(async {
    match Request::post("/api/notes").header("content-type", "application/json").body(r#"{"t":"x"}"#).send().await {
        Ok(r) if r.ok() => { let _ = r.text(); }
        Ok(r) => { /* r.status */ }
        Err(e) => { /* e is FetchError */ }
    }
});
```

## Server-sent events (`src/sse.rs`)

### `SseEvent`

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    Open,
    Message { name: String, data: String },
    Reconnecting,
    Closed,
}
```

* `Open`: the connection is open (also after each automatic reconnect).
* `Message`: an event arrived; `name` is `message` when the server sent none.
* `Reconnecting`: the connection was lost and the browser reconnects by itself (the `EventSource` sends `Last-Event-ID`).
* `Closed`: failed for good (the browser's `readyState` is closed) or closed. Nothing follows.

### `EventSource`

```rust
#[derive(Debug)]
pub struct EventSource { /* id */ }

impl EventSource {
    #[must_use] pub fn open(url: &str, names: &[&str]) -> Self;
    pub async fn next(&self) -> Option<SseEvent>;
    pub fn close(&self);
}
impl Drop for EventSource   // closes and forgets the source
```

* `open(url, names)` connects (same origin, with the page's cookies, as `new EventSource(url)`) and listens for the unnamed event (`message`) and for each name in `names`; the browser cannot report names it was not told. Names containing `\n` are dropped. Names cross as a `\n`-joined string.
* `next()` returns queued events first, in order; waits when none; returns `None` after `Closed` has been delivered or the source was closed/dropped. Events are queued per source, so none is lost while the app is busy. The `Closed` event itself is delivered once (as `Some(SseEvent::Closed)`), then `None`.
* `close()` is idempotent. Events already queued can still be read; then `next` gives `None`. Events arriving after close are ignored.
* An `EventSource` with the URL `quench-bridge:events` is the host bridge's event channel (see [API: bridge](api-bridge.md)).

```rust
let source = EventSource::open("/api/notes/stream", &["note"]);
spawn(async move {
    while let Some(event) = source.next().await {
        if let SseEvent::Message { data, .. } = event { /* ... */ }
    }
});
```
