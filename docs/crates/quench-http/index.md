---
title: quench-http
parent: Crates
nav_order: 24
has_children: true
permalink: /crates/quench-http/
---

`quench-http` (library name `quench_http`) is Quench's HTTP server framework: a router, extractors, middleware, a dependency-injection container, WebSocket and server-sent-event helpers, and an HTTP/1.1 server loop with graceful shutdown. Routes and components are registered by attribute macros (`#[get]`, `#[injectable]`, ...) and discovered at link time, so there is no registration list to maintain. It replaces `actix-web` (`Service`/`Transform` boilerplate, `.service(handler)` lists) in the estate. The protocol layer underneath is `quench-proto` (not a dependency you call directly: `quench_http::http` re-exports it).

* Version: `0.1.15` (`quench-http/Cargo.toml`). Edition and license come from the workspace.
* Companion crate: `quench-http-macros` `0.1.3` (see [quench-http-macros](../quench-http-macros/index.md)). The macros are re-exported from `quench_http::prelude`.
* `autotests = false`, `[lib] harness = false`: all test targets are listed explicitly and use the `quench-test` harness.

## Install

```toml
[dependencies]
quench-http = { registry = "ennor", version = "0.1" }
tokio = { version = "1", features = ["full"] }
```

Inside this workspace the dependency is declared as `quench-http = { path = "quench-http", registry = "ennor", version = "0.1.0", default-features = false }` (root `Cargo.toml`), so a workspace member that writes `quench-http = { workspace = true }` gets **no** `tls` feature unless it adds `features = ["tls"]`.

## Features

| Feature | Default | Enables |
|---|---|---|
| `tls` | yes | `serve_https`, `serve_https_on`, `load_tls`, the `rustls` types in their signatures; pulls in `rustls` and `quench-proto/tls`. |

Without `tls`, `server::serve_*` serve plain HTTP/1.1 only and `rustls` is not built. `quench-proto` is always built with the features `server`, `ws`, `multipart`, `sse`.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `async_trait`, `bytes::Bytes`, `stream`, `registry` (link-time registries), `submit!`, `Error` derive. |
| `quench-http-macros` | `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]`, `#[injectable]`. |
| `quench-proto` (features `server`, `ws`, `multipart`, `sse`; `tls` with the crate feature) | HTTP types, HTTP/1.1 server connection driver, WebSocket, multipart parser, SSE, cookies, TLS acceptor. |
| `quench-serde` | JSON (`json::to_vec`, `json::from_slice`), URL-encoded (`urlencoded::from_str`, `from_bytes`), `Serialize`/`DeserializeOwned`. |
| `quench-log` | `info!`/`warn!`/`debug!` in the server loop. |
| `rustls` (optional, `tls`) | `rustls::ServerConfig` in `serve_https`/`load_tls` signatures. |
| `tokio` | `TcpListener`, `select!`, signals, `mpsc`, `AsyncRead`. |

Dev-dependencies: `quench-test`, `quench-proto` with `client`, `tokio` with `macros`, `rt-multi-thread`. No third-party crate other than `rustls` and `tokio` is used.

## Minimal complete example

```rust
use quench_http::prelude::*;
use std::sync::Arc;

#[get("/")]
async fn hello() -> &'static str {
    "hello from quench-http"
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = "127.0.0.1:8080".parse().unwrap();
    serve_http(router, container, addr).await
}
```

`GET /` answers `200` with `Content-Type: text/plain; charset=utf-8` and the body above. `SIGTERM`/`SIGINT` start a graceful shutdown (see [internals](internals.md)).

## Where to go next

* [Usage](usage.md): routing, extractors, DI, middleware, uploads, streaming, WebSocket, testing handlers.
* API reference: [overview and prelude](api.md), [request, response, errors](api-request-response.md), [extractors, multipart, WebSocket, streams](api-extractors.md), [routing, middleware, server](api-routing-server.md), [dependency injection](api-di.md).
* [Configuration](configuration.md): what can be configured (there are no environment variables).
* [Errors](errors.md): `HttpError`, `ExtractError`, `DiError` and the status code each produces.
* [Internals](internals.md): link-time discovery and its pitfalls, matching rules, shutdown, limits.
* [Testing](testing.md).
* [quench-http-macros](../quench-http-macros/index.md): the attribute macros.
