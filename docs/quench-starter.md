# Quench Starter

`quench-starter` (crate `quench_starter`) is the server bootstrap crate shared by Forge's HTTP services, for both of Forge's HTTP stacks. It wraps the boilerplate every service repeats — TLS/plain HTTP setup with an HTTPS redirect server, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` — behind one `serve()` entry point. Its own README is a short pointer to this page. It is a direct dependency of `conveyor-service`, `switchboard-service`, `sage-service`, `warehouse-service` and `gatehouse-service` (all under `docker/`).

## Two stacks, one shared core

`quench_starter::common::*` (`db::DbWrapper`, `health::HealthState`, `routes::{normalize_base_path, with_base_path}`, `wait::*`) is framework-agnostic and shared by both bootstraps. `quench_starter::actix::*` is the original Actix-web bootstrap; `quench_starter::http::*` is the [`quench-http`](./quench-http.md)-based one, added when Forge started migrating off actix-web. They aren't glob re-exported at the crate root together (`actix::serve` and `http::serve` would collide), so reach the one you want by its full path: `quench_starter::actix::serve` or `quench_starter::http::serve`.

`quench_starter::http::serve` is narrower than the actix version in one respect: it doesn't wire UI login-redirect routes, since those need `quench_auth::http::*` (now available — see [Quench Auth](./quench-auth.md)) wired in by the service itself, the same way `examples/forge_service_demo` does.

## Public API / Key Types

Everything below is reachable through `quench_starter::prelude::*` (the actix stack) unless noted; the quench-http stack is reached via `quench_starter::http::*` directly.

- `actix::serve(root_module, scoped_module, db, init)` — the actix-web entry point. Installs the rustls crypto provider, resolves `BASE_PATH`, builds a `DbWrapper` (unless one is passed in), starts a `HealthState`, wires `FilteredLogger` middleware and the health/swagger/UI routes, then binds either HTTPS + an HTTP→HTTPS redirect server (when `SERVER_CERT_PATH`/`SERVER_KEY_PATH` resolve) or plain HTTP.
- `http::serve(db, init)` — the quench-http equivalent: same `BASE_PATH`/TLS/health/readiness behavior, built on `discover_and_mount` (below) plus the correlation-id/logger middleware, served via `quench_http::server::{serve_http, serve_https}` (graceful shutdown on `SIGTERM`/`SIGINT` included).
- `http::discover_and_mount(base_path)` — the composable half of `http::serve`: discovers every `#[get]`/`#[post]`/... route linked into the binary, mounts it under `base_path`, wraps it in the correlation-id/logger middleware, and returns the `Arc<dyn Endpoint>` — *without* opening a socket or building the DI container, so a service that needs its own middleware (`quench_auth::http::middleware::Auth`, most often) can wrap that around this before serving. `http::serve` itself calls this with nothing extra wrapped, for the common case.
- `ScopedModule` trait — implemented by a service to register its own scope of routes via `register(&self, scope: Scope) -> Scope<...>` (actix stack only; the quench-http stack uses `#[get]`/`#[post]` discovery instead — see [Quench Http](./quench-http.md#routing)).
- `common::db::DbWrapper` — `DbWrapper::init_env()` / `DbWrapper::init(url)` connect through `quench_db::Db::connect`. An empty URL only succeeds when `ALLOW_IN_MEMORY_DB=true`; otherwise the process panics rather than starting against a database it isn't configured for. (`actix::domain::db::DbWrapper` / `http::domain::DbWrapper` both re-export this unchanged.)
- `common::health::HealthState` — `live()`, `is_live()`, `is_ready()`, `mark_ready()`; backs the `/health`, `/health/live`, `/health/ready` routes (actix: mounted by `actix::routers::health::scope()`; quench-http: `#[get]`-discovered from `http::routers::health`).
- `actix::routers::metrics::scope()` — a second, simpler `/metrics`, `/health`, `/health/ready`, `/health/live` set (the metrics text is currently a placeholder gauge, not real Prometheus output; actix stack only).
- `actix::routers::swagger` / `http::routers::swagger` — redirects `/swagger-ui` and `/swagger-ui/` to `/swagger-ui/index.html` under the base path.
- `actix::routers::ui` / `http::routers::ui` — `ui_path`, `ui_asset_path`, `ui_login_redirect` / `ui_login_redirect_for` (htmx-aware, sends `HX-Redirect` for `HX-Request` calls), `serve_assets`, and a re-export of `is_ui_authenticated`. `actix::routers::ui::pages::home::handle_home` and `service_card` help build an authenticated home page (actix stack only).
- `actix::middleware::CorrelationIdMiddleware` / `http::middleware::correlation::CorrelationId` — reads or generates `X-Correlation-ID`, stores it on the request and echoes it on the response. `actix::middleware::logger::FilteredLogger` / `http::middleware::logger::FilteredLogger` logs method/path/status, skipping paths under `LOG_SKIP_PREFIXES` unless the response was an error.
- `common::routes` — `normalize_base_path(raw)`, `with_base_path(path)`.
- `common::wait` — `gatehouse_health_url()`, `wait_for_services(service_name, urls)` (polls each URL until it answers 2xx).
- `logging::init()` — loads `.env` (root, then `docker/<CARGO_BIN_NAME>/.env`), initializes `tracing_subscriber` from `RUST_LOG` (default `info`).
- `metrics::RequestMetrics` / `MetricsSnapshot` / `TimedBlock` — counters for inter-service HTTP calls, with `MetricsSnapshot::to_prometheus(name)` rendering real Prometheus text.
- `resilience::RetryConfig` / `retry_with_backoff` — exponential backoff retry helper; `CircuitBreaker` — closed/open/half-open breaker; `format_error_message` — turns a raw error string into a user-facing message based on keyword sniffing (timeout, connection, rate limit, unauthorized, not found).

Note: `src/tracing.rs` (OpenTelemetry/Jaeger setup, gated behind a `jaeger` feature that isn't declared in `Cargo.toml`) exists in the source tree but is **not** declared as a module in `lib.rs`, so it is not compiled or reachable — treat it as dead code rather than part of the API.

### A registration-order caveat on the quench-http stack

`http::routers::health`/`swagger`/`ui`'s `base_path_redirect`/`base_path_slash_redirect` routes go through the same `#[get]`/`discover_routes()` path as any service's own routes — unlike actix's explicit `.service(...)` call order, there's no *guaranteed* precedence between these and a route a service registers at the same pattern (a root-mounted `{path:.*}` catch-all can match the bare `""`/`"/"` patterns too). In practice `quench-starter`'s own `inventory` registrations link before a dependent service's, so `quench-starter`'s routes win — but that rests on linker behavior, not something enforced by the framework. A service whose own routing needs to win at the literal base path should register more specifically than a bare root wildcard. See `http::routers::ui`'s module doc comment (`src/http/routers/ui.rs`) for the full explanation, and [Quench Http](./quench-http.md#routing) for how registration order works generally.

## Configuration

Read via `envmnt`/`std::env`, all optional unless stated:

- `BASE_PATH` — path prefix every route is scoped under (default `/`).
- `SERVER_ADDR` (default `0.0.0.0:443`), `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`).
- `SERVER_CERT_PATH` / `SERVER_KEY_PATH` — PEM cert/key; when both load successfully, `serve` runs HTTPS plus an HTTP redirect server, otherwise it falls back to plain HTTP.
- `SERVICE_NAME` — used in startup log lines and, via `common::wait`, service identification.
- `DATABASE_URL` / `POSTGRES_URL` — passed to `quench_db::Db::connect`; `ALLOW_IN_MEMORY_DB` must be `true` to allow an empty URL.
- `GATEHOUSE_URL` — base URL `gatehouse_health_url()` appends `/health/ready` to.
- `LOG_SKIP_PREFIXES` — comma-separated path prefixes `FilteredLogger` won't log on success.
- `RUST_LOG` — verbosity passed to `tracing_subscriber` by `logging::init()`.

## Testing

`libs/quench-starter/tests/unit.rs` wires unit tests under `tests/unit/`: `actix_routers_health_tests.rs`, `actix_routers_ui_tests.rs`, `metrics_tests.rs`, `resilience_tests.rs`. The quench-http stack has its own top-level tests (`tests/http_health.rs`, `tests/http_ui_redirects.rs`) plus in-module unit tests for `http::RootOrMounted` (`src/http/mod.rs`), covering the same health/readiness/UI-redirect behavior against `discover_and_mount`.

## Usage example

Actix stack:

```rust
use quench_starter::prelude::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    serve(
        || actix_web::web::scope(""),      // root_module
        || actix_web::web::scope(""),      // scoped_module
        None,                               // db: build from DATABASE_URL
        async {},                           // init: readiness work
    )
    .await
}
```

quench-http stack:

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();

    quench_starter::http::serve(
        None,      // db: build from DATABASE_URL
        async {},  // init: readiness work
    )
    .await
}
```

Every `#[get]`/`#[post]`/`#[injectable]` linked into the binary is discovered automatically — see [Quench Http](./quench-http.md) for routing/DI, and [Example: Forge Service Demo](./examples/forge_service_demo.md) for a full service-shaped app with auth middleware layered on top via `http::discover_and_mount`.

[Home](../README.md)
