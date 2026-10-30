---
title: API - http
parent: quench-starter
grand_parent: Crates
nav_order: 3
---

## `http` (`src/http/mod.rs`)

```rust
pub mod domain;
pub mod middleware;
pub mod routers;

pub async fn serve<I>(db: Option<Arc<DbWrapper>>, init: I) -> std::io::Result<()>
    where I: Future<Output = ()> + Send + 'static;

pub async fn serve_app(service_name: &str, app: Arc<dyn Endpoint>,
                       container: Arc<quench_http::di::Container>) -> std::io::Result<()>;

pub fn discover_and_mount(base_path: impl Into<String>) -> Arc<dyn Endpoint>;
```

| Function | Behaviour |
|---|---|
| `serve(db, init)` | `BASE_PATH` normalised (logged as `Server initialized with BASE_PATH: ..`); database from `db` or `DbWrapper::init_env()`; `HealthState::live()` with `spawn_ready_when(init)`; container `ContainerBuilder::new().provide(db.clone()).provide(health_state).build()` (panic `dependency graph failed to resolve: {e}` on failure); `app = discover_and_mount(base_path)`; `serve_app("{SERVICE_NAME}-service", app, container)`. Never returns normally while the servers run. |
| `serve_app(name, app, container)` | Reads `SERVER_ADDR` (default `0.0.0.0:443`) and `SERVER_HTTP_REDIRECT_ADDR` (default `0.0.0.0:80`); tries `load_tls(SERVER_CERT_PATH or "cert.pem", SERVER_KEY_PATH or "key.pem")`. If it loads: prints `starting HTTPS server on {addr}` and `starting HTTP redirect server on {addr}` and runs `serve_https(app, ..)` and `serve_http(redirect, ..)` together (`tokio::try_join!`; the first error ends both). Else prints a warning `starting plain HTTP server` and runs `serve_http(app, container, SERVER_ADDR)`. |
| `discover_and_mount(base_path)` | Forces the linking of this crate's own routes (`health`, `swagger`, `ui` redirects), discovers all routes (`discover_routes()`), mounts them under `base_path` (`mount`), puts a root handler outside the mount, and wraps in middleware (below). Opens no socket and builds no container. |

Middleware order of `discover_and_mount` (outermost first): `FilteredLogger::default()`, `CorrelationId`, `SecurityHeaders`, then the root handler and the mounted router. Wrap further middleware around the result with `quench_http`'s `wrap` (a layer added later is outer and runs first).

Root handler (private `RootOrMounted`): when `base_path != "/"` and the request is `GET /` exactly, it answers `routers::ui::server_root_redirect()` (302 to the UI); everything else goes to the mounted router. Redirect server (private `RedirectToHttps`): answers every request with `308 Permanent Redirect`, `Location: https://{host}[:{https_port}]{request uri}` where host comes from the `Host` header (port omitted for 443; IPv6 literals keep their brackets); `X-Forwarded-Host` is not honoured.

## `http::domain` (`src/http/domain/`)

```rust
pub mod api_error;
pub mod error;
pub use crate::common::db::DbWrapper;
```

### `api_error`

```rust
#[must_use] pub fn json_error(status: StatusCode, message: &str) -> Response;

#[derive(Debug)]
pub struct ApiError { /* status: StatusCode, message: String */ }
impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self;
    #[must_use] pub fn into_response(self) -> Response;
}
impl quench_http::prelude::IntoResponse for ApiError {}
```

`json_error` answers `status` with `{"error": message}`; if serialization fails, a text response with the message. `ApiError::into_response` logs `api error: {message}` at error level for 5xx, then `json_error`. The `IntoResponse` impl calls the inherent method.

### `error`

```rust
pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const DENIED: &str = "DENIED";
pub const UNSUPPORTED: &str = "UNSUPPORTED";
#[must_use] pub fn response(status: StatusCode, code: &'static str, message: &'static str) -> Response;
#[must_use] pub fn response_with_detail(status: StatusCode, code: &'static str, message: &'static str, detail: Value) -> Response;
```

Body: `{"errors":[{"code": code, "message": message, "detail": detail}]}`; `response` uses `detail = {}`. Falls back to a text response with `message` if serialization fails.

## `http::middleware`

```rust
pub mod correlation; pub mod logger; pub mod security_headers;
```

### `correlation`

```rust
#[derive(Debug)] pub struct CorrelationId;                 // impl Middleware
#[must_use] pub fn inject_correlation_id_header(builder: quench_client::transport::RequestBuilder, correlation_id: &str) -> RequestBuilder;
```

`CorrelationId::handle`: uses the request's `x-correlation-id` header or a new UUID v4; records `correlation_id` on the current `quench_log::Span`; calls next; sets `x-correlation-id` on the response. The id is not stored in the request, handlers cannot read it back. `inject_correlation_id_header` sets that header on an outgoing client request.

### `logger`

```rust
#[derive(Debug, Clone)] pub struct FilteredLogger { /* skip_prefixes: Vec<String> */ }
impl Default for FilteredLogger {}     // prefixes from LOG_SKIP_PREFIXES
impl Middleware for FilteredLogger {}
```

After the response: logs `{method} {path} -> {status}` at info level, unless the status is success **and** the path starts with a skip prefix. `LOG_SKIP_PREFIXES` is split on `,`, trimmed, empties dropped. The query string is not part of the logged path.

### `security_headers`

```rust
#[derive(Debug)] pub struct SecurityHeaders;      // impl Middleware
```

Adds `x-content-type-options: nosniff` to every response that does not already carry that header.

## `http::routers`

```rust
pub mod health; pub mod swagger; pub mod ui;
```

### `health`

```rust
pub use crate::common::health::HealthState;
#[get("/health")]       pub async fn health() -> Response;                       // 200 text "OK"
#[get("/health/live")]  pub async fn live(Inject(state): Inject<HealthState>) -> Response;
#[get("/health/ready")] pub async fn ready(Inject(state): Inject<HealthState>) -> Response;
```

`live`: `200 {"status":"live"}` or `503 {"status":"not_live"}`. `ready`: `200 {"status":"ready"}` or `503 {"status":"not_ready"}`. The state must be in the container, otherwise the extractor fails. Paths are relative to the mount (so `{BASE_PATH}/health`).

### `swagger`

No public items. Registers two routes: `GET /swagger-ui` gives `308` to `with_base_path("/swagger-ui/")`; `GET /swagger-ui/` gives `308` to `with_base_path("/swagger-ui/index.html")`. Nothing serves `index.html`; a service provides its own.

### `ui` (`src/http/routers/ui.rs`)

```rust
pub use quench_auth::http::routers::ui::is_ui_authenticated;
pub mod common;     // pub mod css;
pub mod pages;      // pub mod home;

#[must_use] pub fn ui_path(path: &str) -> String;
#[must_use] pub fn ui_asset_path(path: &str) -> String;
#[must_use] pub fn ui_login_redirect() -> Response;
#[must_use] pub fn ui_login_redirect_for(request: &Request) -> Response;
pub async fn server_root_redirect() -> Response;
#[get("")]  pub async fn base_path_redirect() -> Response;
#[get("/")] pub async fn base_path_slash_redirect() -> Response;
pub async fn serve_assets(path: &str, dist_path: &str) -> Response;
```

| Item | Behaviour |
|---|---|
| `ui_path(path)` | `with_base_path("/ui" + path)`; logs at debug level. `ui_path("/login")` is `{BASE_PATH}/ui/login`. |
| `ui_asset_path(path)` | `ui_path("/assets" + path)`. |
| `ui_login_redirect()` | `302`, `Location: ui_path("/login")`. |
| `ui_login_redirect_for(request)` | If the request has an `hx-request` header: `200` with `HX-Redirect: ui_path("/login")`; else `ui_login_redirect()`. |
| `server_root_redirect()` | `302` to `ui_path("")` (`{BASE_PATH}/ui`). Used by the root handler for `GET /`. |
| `base_path_redirect`, `base_path_slash_redirect` | Routes at the mount's `""` and `"/"` answering `302` to `{BASE_PATH}/ui`. |
| `serve_assets(path, dist_path)` | `path` is made of normal components only (`..`, absolute, empty or `.` components give `400`); reads `dist_path/path` (unreadable: `404`); `200` with `cache-control: public, max-age=3600` and a content type by extension: css `text/css; charset=utf-8`, js `application/javascript; charset=utf-8`, json `application/json; charset=utf-8`, svg `image/svg+xml`, png, jpg/jpeg, ico `image/x-icon`, otherwise `application/octet-stream`. Reads the whole file synchronously. |
| `is_ui_authenticated` | Re-export from `quench-auth`. |

#### `ui::pages::home`

```rust
pub async fn handle_home<F>(req: &Request, config: &JwtConfig, render_fn: F) -> Response where F: FnOnce() -> Response;
#[must_use] pub fn service_card(href: &str, title_key: &str, desc_key: &str, extra_class: &str) -> Element;
```

`handle_home`: if `is_ui_authenticated(req, config)` is false, returns `ui_login_redirect()` (not the htmx variant); else `render_fn()`. `service_card`: `<a href class="home-card {extra_class}"><div class="home-card-body"><div class="home-card-title" data-i18n=title_key/><div class="home-card-desc" data-i18n=desc_key/></div><div class="home-card-arrow">→</div></a>` built with `quench_web` elements.

#### `ui::common::css`

```rust
#[must_use] pub fn layout_rules() -> Vec<CssRule>;
#[must_use] pub fn home_rules() -> Vec<CssRule>;
#[must_use] pub fn login_rules() -> Vec<CssRule>;
#[must_use] pub fn meta_rules() -> Vec<CssRule>;
```

Shared page-shell CSS rules as `quench_web::prelude::CssRule` trees: `layout_rules` (header, `.content`, `.content-inner`, `.page`, `.page-header`, `.split-view` with a one-column rule under 1024 px, `.split-right`, `.right-top`, `.right-bottom`, ...), `home_rules` (home cards), `login_rules` (login page), `meta_rules`. The exact rule contents are in `src/http/routers/ui/common/css.rs`.
