---
title: API - routing, middleware, server
parent: quench-http
grand_parent: Crates
nav_order: 6
---

Modules `route`, `router`, `middleware`, `server`.

## `route`

### `pub struct RouteRegistration` (`#[derive(Debug)]`)

```rust
pub struct RouteRegistration {
    pub method: Method,
    pub pattern: &'static str,
    pub endpoint: fn() -> Arc<dyn Endpoint>,
}
```

Submitted by `#[get]`/`#[post]`/`#[put]`/`#[delete]`/`#[patch]` for each handler. Not meant to be built by hand.

### `pub static ROUTES: Registry<RouteRegistration>`

The link-time registry (`quench_core::registry::Registry`) that holds every registration in the linked binary.

### `pub fn discover_routes() -> Router`  (`#[must_use]`)

Builds a `Router` from all of `ROUTES`, in registry iteration (link) order, calling each `endpoint()` once. Panics if a registered pattern is malformed (unclosed `{`, unsupported constraint).

## `router`

### `pub struct Router` (`#[derive(Debug)]`)

```rust
#[must_use] pub fn builder() -> RouterBuilder
pub async fn dispatch(&self, req: Request) -> Response
```

`dispatch` finds the first pattern (registration order) matching the routed path (URI path minus any `mount` prefix; the query is not part of it), stores the captures on the request, then picks the handler with an equal method. No pattern: `Response::not_found()` (`404`, body `not found`). Pattern matches, method does not: `405`, body `method not allowed` (no `Allow` header). `HEAD` and `OPTIONS` are not derived from `GET`; they answer `405`. `Router` implements `Endpoint` (`call` = `dispatch`).

### `pub struct RouterBuilder` (`#[derive(Debug, Default)]`)

```rust
pub fn route(&mut self, method: Method, pattern: &'static str, endpoint: Arc<dyn Endpoint>) -> &mut Self
#[must_use] pub fn finish(self) -> Router
```

A second `route` call with the **same pattern string** appends a method handler to the existing entry (so the position of the pattern stays that of its first registration); two registrations of the same pattern and method keep both, the first wins. `finish` parses all patterns and panics on a malformed one. `RouterBuilder::default()` is also what `Router::builder()` returns.

### `pub fn mount(prefix: impl Into<String>, inner: Arc<dyn Endpoint>) -> Arc<dyn Endpoint>`

* `prefix` empty or `"/"`: `inner` unchanged.
* Otherwise the request's full `uri().path()` must start with `prefix` and the remainder must be empty or start with `/`; otherwise `404` (`not found`). `/api-legacy/x` does not match a `/api` mount.
* On a match, `skip_prefix(prefix.len())` and `inner.call(req)`.

Caveats (verified against the code, see the report): the check uses the full URI path, so nesting `mount` in `mount` always yields `404` from the inner mount; a prefix with a trailing slash (`"/api/"`) makes every request `404` because the remainder never starts with `/`. Middleware wrapped around the mount (or inside) sees the original, unstripped `uri()`.

## `middleware`

### `pub trait Middleware: Send + Sync + 'static`

```rust
#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response;
}
```

`next.call(req).await` runs the rest of the chain; not calling it short-circuits.

### `pub fn wrap<M: Middleware>(inner: Arc<dyn Endpoint>, middleware: M) -> Arc<dyn Endpoint>`

`wrap(wrap(inner, a), b)` runs `b`, then `a`, then `inner`.

### `pub struct OnPathPrefix<M>` (`#[derive(Debug)]`)

```rust
pub const fn new(prefix: &'static str, inner: M) -> Self
```

`impl<M: Middleware> Middleware for OnPathPrefix<M>`: runs `inner` when `req.uri().path().starts_with(prefix)` (plain string prefix: `/api` also matches `/apix`), otherwise forwards to `next`. Fields are private.

## `server`

```rust
pub const DEFAULT_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(25);
pub const DEFAULT_HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn serve_http(app: Arc<dyn Endpoint>, container: Arc<Container>, addr: SocketAddr) -> std::io::Result<()>
pub async fn serve_http_on(app: Arc<dyn Endpoint>, container: Arc<Container>, listener: TcpListener,
                           shutdown: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()>

#[cfg(feature = "tls")] pub async fn serve_https(app: Arc<dyn Endpoint>, container: Arc<Container>, addr: SocketAddr, config: rustls::ServerConfig) -> std::io::Result<()>
#[cfg(feature = "tls")] pub async fn serve_https_on(app: Arc<dyn Endpoint>, container: Arc<Container>, listener: TcpListener, config: rustls::ServerConfig,
                           shutdown: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()>
#[cfg(feature = "tls")] pub fn load_tls(cert_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Option<rustls::ServerConfig>
```

* `serve_http` / `serve_https` bind `addr` (an `Err` from `bind` is returned) and use the OS-signal shutdown: `SIGINT` (Ctrl+C) or, on Unix, `SIGTERM`. If the SIGTERM handler cannot be installed only Ctrl+C works.
* The `_on` variants take a bound `TcpListener` and any shutdown future, e.g. for tests (bind `127.0.0.1:0`).
* They return `Ok(())` after shutdown; `Err` only for a bind failure. A failed `accept` is logged (`warn`) and retried after 50 ms.
* `load_tls` calls `quench_proto::tls::load_server_config(cert, key)`, returns `None` when that does, and sets ALPN to `["http/1.1"]`.
* Logging: `info` `listening on http://ADDR` / `https://ADDR`, `shutdown signal received, draining connections`, `all connections drained`; `warn` `graceful shutdown timed out after 25s, forcing exit`.

Behaviour details are in [internals](internals.md#server-loop).
