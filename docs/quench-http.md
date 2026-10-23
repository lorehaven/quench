# Quench Http

`quench-http` (crate `quench_http`) is Forge's own HTTP framework — a lightweight, `poem`-shaped HTTP framework. It exists because Forge's services needed three things: middleware and extractors without `Service`/`Transform` boilerplate, dependency injection that resolves a real graph instead of threading `Arc`s through every scope by hand, and route/component *discovery* — annotate a function and it's wired in, no `.service(handler)` list to keep in sync. Routing uses a small built-in matcher (`src/pattern.rs`, no regex dependency) that supports the multi-segment wildcard segments Forge's Docker registry and crates-index endpoints need (`/{name:.+}/blobs/{digest}`) — a plain radix-tree router (what most minimal Rust frameworks use) can't express those.

If you know `poem` or `tower`, the shapes will feel familiar: an `Endpoint` trait instead of `Service`, a `Middleware` trait that's a plain `async fn` and extractors (`Path`, `Query`, `Json`, ...) that work as handler parameters the same way. The two things that don't have a direct equivalent are the annotation-driven discovery (`#[get]`/`#[injectable]`, no manual registration) and the dependency-injection container.

This page is a guide, not just a reference — it builds up feature by feature the way `examples/http_demo` (a runnable version of everything below) does. For the fuller production-service shape — health checks, base-path scoping, TLS, auth middleware — see [Quench Starter](./quench-starter.md) and [Quench Auth](./quench-auth.md), and [Example: Forge Service Demo](./examples/forge_service_demo.md) for how they compose with this crate.

## Features

`tls` (default) enables `serve_https`, `serve_https_on` and `load_tls`, and with them the `rustls` types in their signatures. Without it the server is plain HTTP/1.1 (the usual shape behind an ingress that terminates TLS) and `quench-http` does not build `rustls`, `ring` or `webpki-roots`. `quench-http` always enables `quench-proto`'s `server`, `ws`, `multipart` and `sse`.

```toml
quench-http = { version = "0.1", default-features = false } # plain HTTP only
```

## Streaming bodies

`Response::streaming(status, stream)` takes any `quench_core::stream::Stream` of `Result<Bytes, E>` that is `Send + Sync + 'static` (the `Sync` is needed because middleware holds a `&Response` across awaits). `quench_http::stream` turns tokio sources into such streams: `receiver_stream(mpsc::Receiver<T>)` (everything sent until every sender is gone) and `reader_stream(impl AsyncRead)` (64 KiB chunks; a read error is yielded once and ends the stream). For a broadcast channel or any other async source use `quench_core::stream::unfold`.

## Installation

```toml
[dependencies]
quench-http = { registry = "ennor", version = "0.1" }
tokio = { version = "1", features = ["full"] }
```

Everything public is reachable through `quench_http::prelude::*` — the macros (`get`/`post`/`put`/`delete`/`patch`/`injectable`) included, so one `use` is normally enough.

## Quickstart

```rust
use quench_http::prelude::*;
use std::sync::Arc;

#[get("/")]
async fn hello() -> &'static str {
    "hello from quench-http"
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Discovers every #[get]/#[post]/... linked into this binary - `hello`
    // above is never referenced by name anywhere.
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());

    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = "127.0.0.1:8080".parse().unwrap();
    serve_http(router, container, addr).await
}
```

`cargo run`, then `curl http://127.0.0.1:8080/`. `Ctrl+C` triggers a graceful shutdown (see [Serving your app](#serving-your-app)) instead of dropping connections.

## Routing

A handler is a plain `async fn` annotated with its method and path pattern:

```rust
#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>) -> String {
    format!("user #{id}")
}

#[post("/users")]
async fn create_user(Json(body): Json<NewUser>) -> Json<User> { /* ... */ }
```

`#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` all work the same way. Path patterns are literal text plus `{name}`, `{name:.+}` and `{name:.*}` (any other `:constraint` panics at registration):

| Pattern | Matches |
|---|---|
| `/users` | exactly `/users` |
| `/users/{id}` | one segment, captured as `id` |
| `/users/{id}/posts/{post_id}` | two captures |
| `/assets/{path:.*}` | `path` swallows the rest, including `/`s, and may be empty |
| `/{name:.+}/blobs/{digest}` | `name` swallows one or more `/`-separated segments *before* `/blobs/{digest}` - the docker registry API's shape (`library/ubuntu` as a `name`, not just one segment) |

Every handler discovered anywhere in the linked binary is assembled into one `Router` by `discover_routes()`. A path that matches but not for the requested method answers `405`; a path that matches nothing answers `404`.

When two patterns can both match the same request (a static route and a wildcard covering the same prefix, like a sparse index's `/config.json` next to a catch-all `/{path:.*}`), the first one *registered* wins . Registration order for `#[get]`/... handlers is link order, which is usually source order within one crate; across crates it's link order between them, which isn't something you control directly - see [Quench Starter's registration-order caveat](./quench-starter.md#a-registration-order-caveat-on-the-quench-http-stack) for the concrete case this hits (a service's own catch-all route vs. `quench-starter`'s built-in ones).

### Mounting under a prefix

`#[get]` patterns are `&'static str` literals, so they can't bake in a prefix that's only known at runtime (an env var, say). `mount` handles that instead:

```rust
let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
let app = mount("/api/v1", router);
```

A request outside the prefix 404s before `router` ever sees it; one inside it is dispatched with the prefix stripped, so `router`'s own patterns are written as if they owned the root.

## Extractors

A handler's parameters are each resolved through the `FromRequest` trait, in order, before the body runs. Built in:

| Extractor | Reads | Notes |
|---|---|---|
| `Path<T>` | captured path segments | `T: Deserialize` |
| `Query<T>` | the query string | `T: Deserialize` |
| `Json<T>` | the body, as JSON | buffered up to [`BodyLimit`](#body-size-limits); consumes the body |
| `Bytes` | the raw body | same limit as `Json`; consumes the body |
| `Multipart` | a `multipart/form-data` body | field-by-field (see [Multipart uploads](#multipart-uploads)); consumes the body |
| `Inject<T>` | the DI container | see [Dependency injection](#dependency-injection) |
| `Extension<T>` | per-request data a middleware stashed | see [Middleware](#middleware) |
| `Limited<T, N>` | wraps `Json`/`Bytes` with its own `N`-byte cap | overrides `BodyLimit` for one route |

Only one body-consuming extractor (`Json`, `Bytes`, `Multipart`) per handler - the body can only be read once.

```rust
#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[get("/search")]
async fn search(Query(query): Query<SearchQuery>) -> String {
    format!("searching for {:?}, limit {}", query.q, query.limit.unwrap_or(10))
}
```

### Handler return types

Whatever a handler returns just needs `IntoResponse`, implemented for `Response` itself, `()`, `String`, `&'static str`, `HttpError`, `Json<T>` (serializes `T`), and `Result<T, E>` where both sides implement it - so a fallible handler naturally returns `Result<Json<T>, HttpError>` and `?` on any `FromRequest`/DI error just works:

```rust
#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>, Inject(db): Inject<UserDb>) -> Result<Json<User>, HttpError> {
    let user = db.find(id).await.ok_or_else(|| HttpError::status(http::StatusCode::NOT_FOUND, "no such user"))?;
    Ok(Json(user))
}
```

## Dependency injection

`#[injectable]` registers a constructor function as a component in a real dependency graph - not a service locator, an actual graph: `ContainerBuilder::build()` topologically sorts every discovered component, constructs each exactly once in dependency order, and fails fast (before the server ever binds a socket) on a missing dependency or a cycle, naming exactly which components are involved.

```rust
struct Config { label: &'static str }

#[injectable]
async fn provide_config() -> Config {
    Config { label: "prod" }
}

struct Repository { config: Arc<Config> }

#[injectable]
async fn provide_repository(config: Arc<Config>) -> Repository {
    Repository { config }
}
```

Rules:

- Every parameter must be `Arc<Dep>` - the only dependency shape the container resolves. `Dep` is either another `#[injectable]`'s return type, or something seeded directly (see below).
- The return type (or, for a fallible constructor, the `Ok` side of `Result<T, E>`) is what other components - and handlers, via `Inject<T>` - can depend on.
- A component is built once, lazily ordered by the graph, not by source order.

A value that isn't itself `#[injectable]` - config read from an env var at startup, a database pool, anything constructed once by hand - is seeded with `ContainerBuilder::provide`:

```rust
let container = ContainerBuilder::new()
    .provide(BodyLimit(64 * 1024 * 1024)) // see Body size limits
    .build()
    .await
    .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
```

If the value you're seeding is already behind an `Arc` (a constructor that hands one back directly, like `SessionDb::init()`), use `provide_arc` instead of `provide` - `provide(arc)` would key it on `TypeId::of::<Arc<T>>()`, not `TypeId::of::<T>()`, and no `Arc<T>`-shaped dependency would ever find it:

```rust
let session_db: Arc<SessionDb> = SessionDb::init(store);
let container = ContainerBuilder::new().provide_arc(session_db).build().await?;
```

A handler resolves any of this the same way a component does, via `Inject<T>`:

```rust
#[get("/counter")]
async fn counter(Inject(counter): Inject<Counter>) -> String {
    let n = counter.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    format!("hit {n} times")
}
```

`Inject<T>` and a component's own `Arc<Dep>` parameters resolve from the exact same container - there's no separate "handler DI" and "component DI".

### Discovery and linking

Like routes, `#[injectable]`/`#[get]`/... registrations only exist in a binary that actually links the object code they're compiled into. For a service's own crate this is automatic; for routes/components defined in a *library* crate (the way `quench-starter`'s health/swagger routes are), the dependent binary has to reference *something* real from that crate, or the linker never pulls those registrations in at all - an `.rlib` is a lazily-linked archive, and a member nothing calls into doesn't make the final binary, `#[used]`-marked statics included. `quench_http::route::RouteRegistration`'s own doc comment (in `src/route.rs`) has the full explanation; `quench-starter`'s `force_link()` functions are the pattern a library crate uses to guarantee its own built-in routes are always linked, regardless of what else a dependent happens to reference.

## Middleware

`Middleware` is a plain `async fn` wrapping one `Endpoint` in another - no `Service`/`Transform` pair to implement:

```rust
use async_trait::async_trait;

struct RequestLogger;

#[async_trait]
impl Middleware for RequestLogger {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let response = next.call(req).await;
        quench_log::info!("{method} {path} -> {}", response.status());
        response
    }
}

let app = wrap(router, RequestLogger);
```

`wrap(inner, m)` makes `m` the new outermost layer: `wrap(wrap(inner, a), b)` runs `b`, then `a`, then `inner` - the last `wrap` call is outermost. This matters when ordering is load-bearing, e.g. `quench-auth`'s `Auth`/`RequireWrite` pair - `RequireWrite` reads the `Claims` `Auth` inserts, so `Auth` has to run first, which here means it's the outer layer:

```rust
let app = wrap(routes, RequireWrite::new(config.clone()));
let app = wrap(app, Auth::new(config)); // outermost: runs first
```

Middleware that needs to hand a handler some per-request data - decoded auth claims, a request ID - stashes it via `req.extensions_mut().insert(value)`; a handler reads it back with the `Extension<T>` extractor. This is deliberately distinct from `Inject<T>`: `Extension` is fresh data set on *this* request, `Inject` is a shared service the container resolved once - the same `State` vs `Extension` split other frameworks make.

### Scoping middleware to part of the app

`wrap` applies to everything downstream of it. To apply a middleware to only some paths, wrap conditionally:

```rust
struct OnPathPrefix<M> { prefix: &'static str, inner: M }

#[async_trait]
impl<M: Middleware> Middleware for OnPathPrefix<M> {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if req.uri().path().starts_with(self.prefix) {
            self.inner.handle(req, next).await
        } else {
            next.call(req).await
        }
    }
}

let app = wrap(app, OnPathPrefix { prefix: "/api/", inner: Auth::new(config) });
```

`examples/forge_service_demo` uses exactly this to keep `/health` and `/login` reachable without a token while gating everything under `/api/`.

## Error handling

`HttpError` is what `FromRequest`/DI failures produce and what a fallible handler returns on the `Err` side. `HttpError::status(code, message)` builds one directly; `ExtractError` (invalid path/query/JSON/multipart, body too large, a missing DI component) converts into one automatically via `?`, always as `400 Bad Request`. Anything you return from a handler that implements `IntoResponse` works the same way `Response` itself would - there's no separate "error handler" concept to register.

## Body size limits

`Json`, `Bytes`, and `Multipart` all cap how much they'll buffer, using the same knob: [`BodyLimit`](#dependency-injection), seeded once via `ContainerBuilder::provide`. Nothing provided falls back to `DEFAULT_BODY_LIMIT` (2 MiB).

```rust
let container = ContainerBuilder::new()
    .provide(BodyLimit(1024 * 1024 * 1024)) // 1 GiB, for a service that accepts big uploads
    .build()
    .await?;
```

For the one route that needs a different cap than the rest of the app, wrap the extractor in `Limited<_, N>` instead of raising the app-wide limit:

```rust
#[post("/reports")]
async fn upload_report(Limited(Json(report)): Limited<Json<Report>, 10_000_000>) -> &'static str { /* ... */ }
```

## Multipart uploads

`Multipart` reads a `multipart/form-data` body field by field - no derive macro:

```rust
#[post("/upload")]
async fn upload(mut form: Multipart) -> Result<String, HttpError> {
    let mut note = None;
    let mut file_bytes = None;

    while let Some(field) = form.next_field().await? {
        match field.name() {
            Some("note") => note = Some(field.text().await?),
            Some("file") => file_bytes = Some(field.bytes().await?),
            _ => {}
        }
    }

    Ok(format!("note={note:?}, file={} bytes", file_bytes.map(|b| b.len()).unwrap_or(0)))
}
```

`field.file_name()`/`field.content_type()` are available before consuming the field. The whole-stream size cap is the same `BodyLimit` `Json`/`Bytes` use. A field can also be streamed without buffering: `while let Some(chunk) = field.chunk().await? { ... }` hands over the body piece by piece, which is how an upload is piped to disk. A field you do not read is skipped. The parser itself is `quench_proto::multipart` (see [Quench Proto](./quench-proto.md)).

## Cookies

`quench_http::http::cookie` (from [Quench Proto](./quench-proto.md)) parses and builds cookies: `cookie_value(header, "sid")` for the request side, and `Response::cookie(&cookie)` to add a `Set-Cookie` header.

```rust
use quench_http::http::cookie::{Cookie, SameSite};

let cookie = Cookie::build(("sid", token)).path("/").http_only(true).secure(true).same_site(SameSite::Lax).build();
Response::ok().cookie(&cookie)
```

## WebSocket and server-sent events

Take `WebSocketUpgrade` as a handler parameter, then answer with `on_upgrade`:

```rust
#[get("/socket")]
async fn socket(upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(|mut socket| async move {
        while let Ok(Some(message)) = socket.recv().await {
            if let Message::Text(text) = message {
                let _ = socket.send(Message::Text(format!("echo: {text}"))).await;
            }
        }
    })
}
```

The extractor validates the handshake (a request that is not a valid upgrade is a `400` before the handler runs); `on_upgrade` answers `101 Switching Protocols` and runs the closure with a `WebSocket` once the connection has switched. `recv()` yields `Message::Text`/`Binary`/`Pong`/`Close` (pings are answered for you, fragments are reassembled, text is checked for UTF-8), `send()` writes a message and `close(code, reason)` starts the closing handshake. `.config(Config { max_message_size, max_frame_size })` sets the size limits (16 MiB by default; an oversized message closes the connection with `1009`). From a hand-written `Endpoint`, `ws::upgrade(&req, handler)` does the same given the `Request`.

`Response::sse(stream)` streams server-sent events: the stream's items are `quench_proto::sse::Event` values (`Event::new().event("tick").data("...")`), sent with `content-type: text/event-stream` and the headers that stop proxies buffering them. `Response::streaming(status, stream)` is the lower-level form for any chunked body.

## Serving your app

```rust
serve_http(app, container, "0.0.0.0:8080".parse().unwrap()).await
```

or, over TLS:

```rust
if let Some(config) = load_tls("cert.pem", "key.pem") {
    serve_https(app, container, addr, config).await
} else {
    serve_http(app, container, addr).await
}
```

Both run until a `SIGTERM`/`SIGINT` (or, on non-Unix, Ctrl+C) arrives, then stop accepting new connections and wait up to `DEFAULT_GRACEFUL_TIMEOUT` (25s, comfortably under Kubernetes' default `terminationGracePeriodSeconds`) for in-flight ones to finish before returning - a `docker stop`/`kubectl delete pod` drains requests instead of cutting them off. A connection also has a header-read timeout (30s by default) so a client that never finishes sending its request headers can't hold a slot open forever. The server speaks HTTP/1.1 only (keep-alive, pipelining, chunked bodies, `Expect: 100-continue`); TLS is terminated at the ingress in practice, and `serve_https` is there for development and for services that terminate it themselves. The protocol work lives in [Quench Proto](./quench-proto.md), which also rejects the malformed framing request-smuggling relies on.

`serve_http_on`/`serve_https_on` take an already-bound `TcpListener` and an arbitrary shutdown future instead of binding `addr` and always waiting for a real signal - what a test needs to discover an ephemeral port and trigger shutdown itself, and also what lets a service compose its own shutdown source.

## Testing your handlers

Nothing above needs a running server to test - an `Endpoint` is just `async fn call(&self, req: Request) -> Response`, so a test builds a `Request` directly and calls it:

```rust
let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
let req = Request::new(
    Method::GET,
    "/users/42".parse().unwrap(),
    HeaderMap::new(),
    InboundBody::from_bytes(Bytes::new()),
    container,
);
let resp = router.call(req).await;
assert_eq!(resp.status(), StatusCode::OK);
```

This is how `quench-http`'s own test suite is written throughout - see `tests/router_wildcard.rs` and `tests/dependency_graph.rs` for the pattern applied to routing and DI respectively. The one exception is `tests/graceful_shutdown.rs`, which deliberately goes through a real socket and a real HTTP client, because shutdown behavior only exists at that layer.

## Where quench-http stops - and what plugs the rest in

`quench-http` itself has no opinion on health checks, base-path scoping from an env var, TLS file loading conventions, or auth - those are [Quench Starter](./quench-starter.md) (the bootstrap: `serve()`, health/readiness, base-path scoping, the correlation-id/logger middleware, `discover_and_mount()` for a service that needs to add its own middleware around all of that) and [Quench Auth](./quench-auth.md) (`Auth`/`RequireWrite` middleware, JWT/session verification) respectively. `examples/forge_service_demo` shows the three composed together; `examples/http_demo` covers everything on this page in one runnable service.

## Testing

`tests/*.rs` covers the router (including the wildcard cases above), the DI container (chain resolution, missing dependency, cycle detection, `provide_arc`), extensions/headers, body limits, multipart, `mount`, every extractor and response type (`tests/framework.rs`), and - over real sockets - graceful shutdown, HTTPS with a real handshake, WebSocket upgrades, server-sent events and multipart uploads (`tests/e2e.rs`). Run with `cargo test -p quench-http`.

[Home](../README.md)
