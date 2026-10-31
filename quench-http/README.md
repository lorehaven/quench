# quench-http
Lightweight HTTP framework with annotation-driven discovery and dependency injection.

## What it is
`quench-http` is Quench's HTTP server framework: a router, extractors, middleware, a dependency-injection container, WebSocket and server-sent-event helpers, and an HTTP/1.1 server loop with graceful shutdown. Routes and components are registered by attribute macros (`#[get]`, `#[injectable]`, ...) and discovered at link time, so there is no registration list to maintain.

It replaces `actix-web`. The protocol layer underneath is `quench-proto`, re-exported as `quench_http::http`; the macros come from `quench-http-macros`.

## Use cases
- Build JSON or HTML services with typed `Path`, `Query`, `Json` and `Form` extractors.
- Share components between handlers with `#[injectable]` and `Inject`.
- Add cross-cutting behaviour with `Middleware` and `wrap`.
- Handle uploads (`Multipart`), WebSockets and server-sent events.
- Serve plain HTTP or HTTPS with graceful shutdown.

## Example
```rust
use quench_http::prelude::*;
use std::sync::Arc;

#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>) -> String {
    format!("user #{id}")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = "127.0.0.1:8080".parse().unwrap();
    serve_http(router, container, addr).await
}
```

## Features
- `tls` (default): `serve_https`, `serve_https_on`, `load_tls` and the `rustls` types in their signatures.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-http/
