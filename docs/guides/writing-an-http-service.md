---
title: Writing an HTTP service
parent: Guides
nav_order: 5
---

This guide builds a small service with `quench-http`: routes, extractors, DI, middleware, errors, a runtime base path and optional TLS. Reference: [quench-http](../crates/quench-http/index.md), [macros](../crates/quench-http-macros/index.md).

## Dependencies

```toml
[dependencies]
quench-http = { version = "*", features = ["tls"] }   # "tls" only if you call serve_https/load_tls
quench-core = "*"
quench-serde = "*"
quench-log = "*"
tokio = { version = "*", features = ["rt-multi-thread", "macros"] }
```

Use the version your workspace pins. The workspace declares `quench-http` with `default-features = false`, so enable `tls` explicitly.

## Routes

`#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` take one string literal, the path pattern. Handlers are `async fn`. Routes register themselves at link time; `discover_routes()` builds the router. Patterns: `{name}` one segment, `{name:.+}` one or more characters including `/`, `{name:.*}` zero or more. Other regex constraints panic at startup. Full attribute reference: [quench-http-macros API](../crates/quench-http-macros/api.md).

## Extractors

| Extractor | Reads | Failure |
|---|---|---|
| `Path<T>` | path parameters, `T: Deserialize` | 400 `invalid path parameter: ...` |
| `Query<T>` | query string | 400 |
| `Json<T>` | JSON body (also a response type) | 400 (also when over the body limit) |
| `Form<T>` | urlencoded body | 400 |
| `Bytes`, `Limited<E, N>` | raw body, per-route limit | 400 |
| `Multipart` | multipart stream | 400 |
| `Extension<T>` | value a middleware put in the request extensions (`T: Clone`) | 500 if missing |
| `Inject<T>` | DI container value | 500 if missing |

Only one body-consuming extractor per handler. Exact signatures: [extractors](../crates/quench-http/api-extractors.md).

## Errors

A handler may return anything implementing `IntoResponse`, including `Result<T, HttpError>`. `HttpError` converts to a response with a status; `ExtractError` (extractor failure) and `DiError` (graph failure at startup) are separate types. A path that matches nothing is 404; a path that matches under another method is 405. See [errors](../crates/quench-http/errors.md).

## Complete example

Assembled from `examples/http_demo/src/main.rs` and `quench-http/tests/route_macro.rs` (both build and run in the workspace). *Unverified:* this exact combined listing was not compiled as one unit.

```rust
use quench_core::async_trait;
use quench_http::prelude::*;
use quench_serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

struct Counter(AtomicU64);

#[injectable]
async fn provide_counter() -> Counter {
    Counter(AtomicU64::new(0))
}

#[get("/")]
async fn hello() -> &'static str {
    "hello"
}

#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>) -> String {
    format!("user #{id}")
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[get("/search")]
async fn search(Query(query): Query<SearchQuery>) -> String {
    format!("{:?} limit {}", query.q, query.limit.unwrap_or(10))
}

#[derive(Deserialize)]
struct EchoRequest {
    message: String,
}

#[derive(Serialize)]
struct EchoResponse {
    you_said: String,
}

#[post("/echo")]
async fn echo(Json(body): Json<EchoRequest>) -> Json<EchoResponse> {
    Json(EchoResponse { you_said: body.message })
}

#[get("/counter")]
async fn counter(Inject(counter): Inject<Counter>) -> String {
    format!("hit {}", counter.0.fetch_add(1, Ordering::Relaxed) + 1)
}

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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_log::init_console();

    let container = ContainerBuilder::new()
        .provide(BodyLimit(2 * 1024))
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    // Optional runtime base path ("" mounts at the root).
    let base = std::env::var("BASE_PATH").unwrap_or_default();
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let app = wrap(mount(base, router), RequestLogger);

    let addr = "127.0.0.1:8080".parse().unwrap();
    serve_http(app, container, addr).await
}
```

Run the shipped version with `cargo run -p quench-example-http` and the `curl` lines in its header comment.

## Middleware

Implement `Middleware` (`async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response`) and attach it with `wrap(endpoint, middleware)`. The last `wrap` is the outermost layer. `OnPathPrefix::new(prefix, inner)` restricts a middleware to a prefix. Pass per-request data to handlers with `req.extensions_mut().insert(value)` and `Extension<T>`.

## Base path

`mount(prefix, endpoint)` serves the endpoint under a runtime prefix; `""` and `"/"` leave it at the root; requests outside the prefix get 404. Known problems (nested `mount`, a trailing slash in the prefix) are in the [gap report](../crates/quench-http/internals.md) and `docs/_reports/network-and-data-http.md`: use one `mount`, no trailing slash.

## TLS

With the `tls` feature: `load_tls(cert_path, key_path)` returns `Option<_>` (`None` if the files are missing or unreadable), then `serve_https(app, container, addr, tls_config)`. Fall back to `serve_http` in development. For tests, `serve_http_on(app, container, listener, shutdown_future)` takes a pre-bound listener. Signatures: [routing and server](../crates/quench-http/api-routing-server.md).

## Authentication

Add `quench-auth` middleware (`Auth`, `RequireWrite`) as described in [Authentication](authentication.md). Shutdown is graceful: in-flight requests drain for up to `DEFAULT_GRACEFUL_TIMEOUT`.
