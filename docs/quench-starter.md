# Quench Starter

`quench-starter` (crate `quench_starter`) is the server bootstrap crate shared by Forge's HTTP services. It wraps the boilerplate every service repeats — TLS/plain HTTP setup with an HTTPS redirect server, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` — behind one `serve()` entry point. Its own README is a short pointer to this page. It is a direct dependency of `conveyor-service`, `switchboard-service`, `sage-service`, `warehouse-service`, `workbench-service` and `gatehouse-service` (all under `docker/`) — every one of which now builds on the `quench-http` stack, `serve_app` in particular, since none of them can use plain `serve` (each needs its own DI providers and/or auth middleware).

## Layout

`quench_starter::common::*` (`db::DbWrapper`, `health::HealthState`, `routes::{normalize_base_path, with_base_path}`, `wait::*`) is framework-agnostic. `quench_starter::http::*` is the [`quench-http`](./quench-http.md)-based bootstrap and is glob re-exported at the crate root (`quench_starter::http::serve` is also `quench_starter::serve`).

`http::serve` doesn't wire UI login-redirect routes, since those need `quench_auth::http::*` (see [Quench Auth](./quench-auth.md)) wired in by the service itself, the same way `examples/forge_service_demo` does.

## Public API / Key Types

Everything below is reachable through `quench_starter::prelude::*` or `quench_starter::http::*`.

- `http::serve(db, init)` — same `BASE_PATH`/TLS/health/readiness behavior, built on `discover_and_mount` (below) plus the correlation-id/logger middleware, and `serve_app` (below) for the TLS-detecting dispatch. Covers the common case that needs no extra DI providers or middleware.
- `http::serve_app(service_name, app, container)` — the TLS-detecting tail end of `http::serve`, factored out for a service that can't call `serve` directly because it needs its own DI providers or middleware (`quench_auth::http::middleware::Auth`, most often): HTTPS + an HTTP→HTTPS redirect server when `SERVER_CERT_PATH`/`SERVER_KEY_PATH` resolve, plain HTTP on `SERVER_ADDR` otherwise, served via `quench_http::server::{serve_http, serve_https}`. Pair it with `discover_and_mount` (below) and your own `ContainerBuilder`:
  ```rust
  let app = quench_starter::http::discover_and_mount(base_path);
  let app = wrap(app, Auth::new(config));
  let container = Arc::new(ContainerBuilder::new().provide(extra).build().await?);
  quench_starter::http::serve_app("my-service", app, container).await
  ```
- `http::discover_and_mount(base_path)` — the composable half of `http::serve`: discovers every `#[get]`/`#[post]`/... route linked into the binary, mounts it under `base_path`, wraps it in the correlation-id/logger middleware, and returns the `Arc<dyn Endpoint>` — *without* opening a socket or building the DI container, so a service that needs its own middleware can wrap that around this before calling `serve_app`. `http::serve` itself calls this with nothing extra wrapped, for the common case.
- `common::db::DbWrapper` — `DbWrapper::init_env()` / `DbWrapper::init(url)` connect through `quench_db::Db::connect`. An empty URL only succeeds when `ALLOW_IN_MEMORY_DB=true`; otherwise the process panics rather than starting against a database it isn't configured for. (`http::domain::DbWrapper` re-exports it.)
- `common::health::HealthState` — `live()`, `is_live()`, `is_ready()`, `mark_ready()`, `spawn_ready_when(init)` (spawns `init`, then marks ready once it completes — the one place `mark_ready`/"initialization complete" logging lives, instead of every service hand-rolling its own `tokio::spawn` wrapper around a dependency wait); backs the `/health`, `/health/live`, `/health/ready` routes (`#[get]`-discovered from `http::routers::health`).
- `http::routers::swagger` — redirects `/swagger-ui` and `/swagger-ui/` to `/swagger-ui/index.html` under the base path.
- `http::routers::ui` — `ui_path`, `ui_asset_path`, `ui_login_redirect` / `ui_login_redirect_for` (htmx-aware, sends `HX-Redirect` for `HX-Request` calls), `serve_assets`, and a re-export of `is_ui_authenticated`. `http::routers::ui::pages::home::{handle_home, service_card}` help build an authenticated home page, and `http::routers::ui::common::css` holds the shared page-shell CSS rules.
- `http::middleware::correlation::CorrelationId` — reads or generates `X-Correlation-ID`, stores it on the request and echoes it on the response. `http::middleware::logger::FilteredLogger` logs method/path/status, skipping paths under `LOG_SKIP_PREFIXES` unless the response was an error.
- `common::routes` — `normalize_base_path(raw)`, `with_base_path(path)`.
- `common::wait` — `gatehouse_health_url()`, `wait_for_services(service_name, urls)` (polls each URL until it answers 2xx).
- `logging::init()` — loads `.env` (root, then `docker/<CARGO_BIN_NAME>/.env`), installs [Quench Log](./quench-log.md) (`quench_log::init()`), which reads `RUST_LOG` (default `info`) and the other `LOG_*` variables.
- `metrics::RequestMetrics` / `MetricsSnapshot` / `TimedBlock` — counters for inter-service HTTP calls, with `MetricsSnapshot::to_prometheus(name)` rendering real Prometheus text.
- `resilience::RetryConfig` / `retry_with_backoff` — exponential backoff retry helper; `CircuitBreaker` — closed/open/half-open breaker; `format_error_message` — turns a raw error string into a user-facing message based on keyword sniffing (timeout, connection, rate limit, unauthorized, not found).


### A registration-order caveat on the quench-http stack

`http::routers::health`/`swagger`/`ui`'s `base_path_redirect`/`base_path_slash_redirect` routes go through the same `#[get]`/`discover_routes()` path as any service's own routes — there's no *guaranteed* precedence between these and a route a service registers at the same pattern (a root-mounted `{path:.*}` catch-all can match the bare `""`/`"/"` patterns too). In practice `quench-starter`'s own registrations link before a dependent service's, so `quench-starter`'s routes win — but that rests on linker behavior, not something enforced by the framework. A service whose own routing needs to win at the literal base path should register more specifically than a bare root wildcard. See `http::routers::ui`'s module doc comment (`src/http/routers/ui.rs`) for the full explanation, and [Quench Http](./quench-http.md#routing) for how registration order works generally.

## Configuration

Read via `envmnt`/`std::env`, all optional unless stated:

- `BASE_PATH` — path prefix every route is scoped under (default `/`).
- `SERVER_ADDR` (default `0.0.0.0:443`), `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`).
- `SERVER_CERT_PATH` / `SERVER_KEY_PATH` — PEM cert/key; when both load successfully, `serve` runs HTTPS plus an HTTP redirect server, otherwise it falls back to plain HTTP.
- `SERVICE_NAME` — used in startup log lines and, via `common::wait`, service identification.
- `DATABASE_URL` / `POSTGRES_URL` — passed to `quench_db::Db::connect`; `ALLOW_IN_MEMORY_DB` must be `true` to allow an empty URL.
- `GATEHOUSE_URL` — base URL `gatehouse_health_url()` appends `/health/ready` to.
- `LOG_SKIP_PREFIXES` — comma-separated path prefixes `FilteredLogger` won't log on success.
- `RUST_LOG` — verbosity read by `quench_log` through `logging::init()`.

## Testing

`quench-starter/tests/unit.rs` wires unit tests under `tests/unit/` (`common_*`, `http_routers_ui_*`, `metrics_tests.rs`, `resilience_tests.rs`). Top-level tests (`tests/http_health.rs`, `tests/http_ui_redirects.rs`) plus in-module unit tests for `http::RootOrMounted` (`src/http/mod.rs`), covering the same health/readiness/UI-redirect behavior against `discover_and_mount`.

## Usage example

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
