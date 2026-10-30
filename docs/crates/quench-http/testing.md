---
title: Testing
parent: quench-http
grand_parent: Crates
nav_order: 10
---

## Testing code built on quench-http

An `Endpoint` is `async fn call(&self, req: Request) -> Response`, so most tests need no socket:

```rust
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::prelude::*;
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use std::sync::Arc;

#[quench_test::test]
async fn get_user_works() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let router = discover_routes();
    let req = Request::new(
        Method::GET,
        "/users/42".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = router.call(req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_inner().body.collect_limited(1 << 20).await.unwrap();
    assert_eq!(&body[..], b"user #42");
}
```

For a real socket use `serve_http_on(app, container, TcpListener::bind("127.0.0.1:0").await?, shutdown_future)` and a client (`quench-proto` with `client`, as the crate's own `e2e` tests do). Seed test-only values with `ContainerBuilder::provide`.

## How quench-http is tested

Every target is `harness = false` and ends with `quench_test::main!()` (Cargo.toml `[[test]]` entries; `autotests = false`). Run: `cargo test -p quench-http`. Unit tests live in `src/pattern.rs`, `src/path_de.rs`, `src/stream.rs`.

| Target | Covers |
|---|---|
| `body_limits` | default 2 MiB limit, provided `BodyLimit`, `Bytes`, `Limited` |
| `dependency_graph`, `di_cycle`, `di_missing_dependency`, `di_provide_arc`, `di_provided_value` | DI chains, cycle and missing errors, `provide`/`provide_arc` |
| `e2e` | plain HTTP round trip and keep-alive, 404, SSE, multipart upload, WebSocket (valid and not), HTTPS with a real handshake, shutdown with idle connections |
| `extensions_and_headers` | `Extension`, `append_header` |
| `form` | `Form` decoding and malformed body |
| `framework` | extractors, handler result conversions, response builders, streaming, router method/path selection, mount, `serve_http` on SIGTERM, `serve_https`/`load_tls`, bind failure |
| `graceful_shutdown` | an in-flight request survives shutdown, new connections refused afterwards |
| `mount_prefix`, `on_path_prefix` | `mount` and `OnPathPrefix` |
| `multipart` | fields, missing boundary, whole-stream limit |
| `route_macro` | `#[get]` + `Path` + `Inject` discovery |
| `router_wildcard` | multi-segment patterns, 404/405, registration order |
| `security_http` | the properties listed in [internals](internals.md#security-relevant-properties-tested-in-testssecurity_httprs) |
| `streaming` | chunked streaming, mid-stream error |

Not covered by any test (found while documenting): nested `mount`, `mount` with a trailing slash, two `#[injectable]` functions returning one type, `HEAD` requests.

The result of `cargo test --offline -p quench-http` at the time of writing: all targets passed.
