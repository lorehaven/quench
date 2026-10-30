---
title: API overview
parent: quench-http
grand_parent: Crates
nav_order: 2
---

Public items of `quench_http` (`quench-http/src/lib.rs`), in source order. The detail pages: [request, response, errors](api-request-response.md), [extractors, multipart, WebSocket, streams](api-extractors.md), [routing, middleware, server](api-routing-server.md), [dependency injection](api-di.md).

## Crate-root re-exports

```rust
pub use quench_core::async_trait;
pub use quench_core::registry;
pub use quench_core::submit;
pub use quench_proto as http;
```

They exist so macro-generated code needs only `quench-http` as a dependency. `quench_http::http` is the whole `quench-proto` crate (`Method`, `StatusCode`, `Uri`, `HeaderMap`, `Extensions`, `cookie`, `sse`, ...).

## Modules

| Module | Contents |
|---|---|
| `body` | `InboundBody` |
| `di` | `Container`, `ContainerBuilder`, `ComponentFactory`, `COMPONENTS`, `BoxedAny`, `ConstructFuture`, re-export `DiError` |
| `endpoint` | `Endpoint`, `IntoResponse` |
| `error` | `DiError`, `IntoHttpError`, `HttpError`, `ExtractError` |
| `extract` | `FromRequest`, `Path`, `Query`, `Json`, `Form`, `Bytes`, `Limited`, `Extension`, `Inject`, `BodyLimit`, `DEFAULT_BODY_LIMIT` |
| `middleware` | `Middleware`, `wrap`, `OnPathPrefix` |
| `multipart` | `Multipart`, `Field` |
| `request` | `Request` |
| `response` | `Response` |
| `route` | `RouteRegistration`, `ROUTES`, `discover_routes` |
| `router` | `Router`, `RouterBuilder`, `mount` |
| `server` | `serve_http`, `serve_http_on`, `serve_https`, `serve_https_on`, `load_tls`, `DEFAULT_GRACEFUL_TIMEOUT`, `DEFAULT_HEADER_READ_TIMEOUT` |
| `stream` | `receiver_stream`, `reader_stream` |
| `ws` | `upgrade`, `upgrade_with`, `WebSocketUpgrade`, re-exports `CloseFrame`, `Config`, `Error`, `Message`, `WebSocket` |
| `prelude` | see below |

`path_de` and `pattern` are private modules (see [internals](internals.md)).

## `prelude`

```rust
pub use crate::di::{Container, ContainerBuilder};
pub use crate::endpoint::{Endpoint, IntoResponse};
pub use crate::error::{DiError, ExtractError, HttpError};
pub use crate::extract::{BodyLimit, Bytes, Extension, Form, FromRequest, Inject, Json, Limited, Path, Query};
pub use crate::http;
pub use crate::middleware::{Middleware, OnPathPrefix, wrap};
pub use crate::multipart::{Field, Multipart};
pub use crate::request::Request;
pub use crate::response::Response;
pub use crate::route::{RouteRegistration, discover_routes};
pub use crate::router::{Router, RouterBuilder, mount};
pub use crate::server::{DEFAULT_GRACEFUL_TIMEOUT, serve_http, serve_http_on};
#[cfg(feature = "tls")]
pub use crate::server::{load_tls, serve_https, serve_https_on};
pub use quench_http_macros::{delete, get, injectable, patch, post, put};
```

Not in the prelude: `WebSocketUpgrade` and the `ws` items, `InboundBody`, `DEFAULT_BODY_LIMIT`, `DEFAULT_HEADER_READ_TIMEOUT`, `IntoHttpError`, `stream::*`, `di::{ComponentFactory, COMPONENTS, BoxedAny, ConstructFuture}`, `route::ROUTES`, `async_trait`. Note that `prelude::Bytes` is the **extractor** `quench_http::extract::Bytes`, not `quench_core::bytes::Bytes`.

## Public item count

| Kind | Count |
|---|---|
| Modules (public, excluding `prelude`) | 14 |
| Structs / enums | 25 (`InboundBody`, `ComponentFactory`, `Container`, `ContainerBuilder`, `DiError`, `HttpError`, `ExtractError`, `Path`, `Query`, `BodyLimit`, `Json`, `Form`, `Bytes`, `Limited`, `Extension`, `Inject`, `OnPathPrefix`, `Multipart`, `Field`, `Request`, `Response`, `RouteRegistration`, `Router`, `RouterBuilder`, `WebSocketUpgrade`) |
| Traits | 5 (`Endpoint`, `IntoResponse`, `IntoHttpError`, `FromRequest`, `Middleware`) |
| Free functions | 12 (`wrap`, `discover_routes`, `mount`, `serve_http`, `serve_http_on`, `serve_https`, `serve_https_on`, `load_tls`, `receiver_stream`, `reader_stream`, `upgrade`, `upgrade_with`) |
| Constants / statics | 5 (`COMPONENTS`, `ROUTES`, `DEFAULT_BODY_LIMIT`, `DEFAULT_GRACEFUL_TIMEOUT`, `DEFAULT_HEADER_READ_TIMEOUT`) |
| Type aliases | 2 (`BoxedAny`, `ConstructFuture`) |
| Macros | 6 attribute macros (re-exported) + the re-exported `submit!` |
