---
title: quench-starter
parent: Crates
nav_order: 44
has_children: true
permalink: /crates/quench-starter/
---

`quench-starter` (library `quench_starter`) is the server bootstrap crate shared by the Forge HTTP services. It wraps what every service repeats: HTTPS with an HTTP-to-HTTPS redirect server (or plain HTTP), base-path scoping, health and readiness routes, request logging, correlation ids, security headers, database bootstrap via `quench-db`, logging initialisation, and a few helpers (service wait, retries, circuit breaker, request metrics), behind one entry point, `http::serve`.

* Version: `0.4.19` (`quench-starter/Cargo.toml`). `[lib] harness = false`.
* No `[features]` table.

## Install

```toml
[dependencies]
quench-starter = "0.4"
```

## Dependencies

| Dependency (features) | Why |
|---|---|
| `quench-http` (`tls`) | `Endpoint`, `Middleware`, routes (`#[get]`), `ContainerBuilder`, `mount`, `discover_routes`, `serve_http`, `serve_https`, `load_tls`. |
| `quench-auth` (`http`, `redis`, `postgres`, `tls`) | `is_ui_authenticated`, `JwtConfig` (UI pages). |
| `quench-db` (`postgres`, `tls`) | `Db::connect`. |
| `quench-cli` (`repl`) | `print_status`, `Tone`. |
| `quench-core` | `env`, `uuid`, `bytes`, `async_trait`. |
| `quench-serde` | `json!`, `Serialize`. |
| `quench-log` | `info!`, `init()`. |
| `quench-web` | `CssRule`, `Element`, `a()`, `div()` for UI helpers. |
| `quench-client` (`tls`) | HTTP client for `wait_for_services` and header injection. |
| `tokio` | `spawn`, `sleep`, `try_join!`. |

Dev-dependency: `quench-test`.

## Minimal example

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    quench_starter::http::serve(
        None,      // db: built from DATABASE_URL / POSTGRES_URL
        async {},  // init: readiness work; /health/ready answers 200 when it completes
    )
    .await
}
```

Every `#[get]`/`#[post]`/`#[injectable]` linked into the binary is discovered automatically.

## Where to go next

* [Usage](usage.md): `serve`, `serve_app` with auth middleware, base paths, readiness, retries.
* [API: common, prelude, root](api.md), [API: http](api-http.md), [API: logging, metrics, resilience](api-support.md).
* [Configuration](configuration.md): environment variables and defaults.
* [Errors](errors.md): panics and error responses.
* [Internals](internals.md): composition order, route linking caveat, redirects.
* [Testing](testing.md).
