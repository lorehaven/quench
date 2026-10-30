---
title: API - root, prelude, common
parent: quench-starter
grand_parent: Crates
nav_order: 2
---

Other API pages: [http](api-http.md), [logging, metrics, resilience](api-support.md). The crate contains no `unsafe`.

## Crate root (`src/lib.rs`)

```rust
pub mod common;
pub mod http;
pub mod logging;
pub mod metrics;
pub mod prelude;
pub mod resilience;

pub use common::*;       // the modules db, format, health, routes, wait
pub use http::*;         // domain, middleware, routers, serve, serve_app, discover_and_mount
pub use logging::*;      // init
pub use metrics::*;      // RequestMetrics, MetricsSnapshot, TimedBlock
pub use resilience::*;   // RetryConfig, retry_with_backoff, format_error_message, CircuitBreaker, CircuitState
```

Glob re-exports of `common` and `http` re-export their **modules and functions**, not the items inside `common::*` submodules: `quench_starter::normalize_base_path` does not exist; use `quench_starter::routes::normalize_base_path` or the prelude. `logging::init` is also reachable as `quench_starter::init`.

## `prelude` (`src/prelude.rs`)

```rust
pub use crate::common::db::DbWrapper;
pub use crate::common::format::human_bytes;
pub use crate::common::health::HealthState;
pub use crate::common::routes::{normalize_base_path, with_base_path};
pub use crate::common::wait::{gatehouse_health_url, wait_for_services};
pub use crate::http::domain::api_error::{ApiError, json_error};
pub use crate::http::domain::error;
pub use crate::http::serve;
pub mod routers { pub use crate::http::routers::*; }
```

## `common::db` (`src/common/db.rs`)

```rust
#[derive(Debug)]
pub struct DbWrapper { pub db: Db }          // quench_db::prelude::Db

impl DbWrapper {
    pub async fn init_env() -> Arc<Self>;
    pub async fn init(url: impl ToString) -> Arc<Self>;
}
```

* `init_env()`: URL from `DATABASE_URL`, else `POSTGRES_URL`, else empty (`quench_core::env::get_any`); then `init`.
* `init(url)`: panics when the trimmed URL is empty and `ALLOW_IN_MEMORY_DB` is not true (an empty URL would otherwise silently give an in-memory database). Calls `Db::connect(&url)`; panics on failure (`configured database connection failed: {e}`). Prints a status line: `Success db connected to Postgres` for `Db::Postgres`, or a `Warn` line for `Db::InMemory`. It does **not** migrate the schema and has no recreate switch (schema lifecycle belongs to foundry).
* `http::domain::DbWrapper` re-exports it.

## `common::format` (`src/common/format.rs`)

```rust
#[must_use] pub fn human_bytes(bytes: i64) -> String;
```

`>= 1 MiB`: `"{:.1} MB"` (`"3.4 MB"`); `>= 1 KiB`: `"{:.0} KB"` (`"7 KB"`); otherwise `"{bytes} B"` (negative numbers print as-is, e.g. `"-5 B"`). Uses 1024 as the base.

## `common::health` (`src/common/health.rs`)

```rust
#[derive(Debug, Clone, Default)]
pub struct HealthState { /* live: Arc<AtomicBool>, ready: Arc<AtomicBool> */ }

impl HealthState {
    #[must_use] pub fn live() -> Self;
    #[must_use] pub fn is_live(&self) -> bool;
    #[must_use] pub fn is_ready(&self) -> bool;
    pub fn mark_ready(&self);
    pub fn spawn_ready_when<F>(&self, init: F) where F: Future<Output = ()> + Send + 'static;
}
```

* `live()`: live = true, ready = false. Note `HealthState::default()` gives live = false, ready = false (so `/health/live` answers 503).
* There is no way to mark not-live or not-ready again.
* `spawn_ready_when(init)`: `tokio::spawn`s `init`, then `mark_ready()` and logs `Service initialization complete`. Needs a Tokio runtime. Clones share state.

## `common::routes` (`src/common/routes.rs`)

```rust
#[must_use] pub fn normalize_base_path(raw: &str) -> String;
pub fn with_base_path(path: &str) -> String;
```

* `normalize_base_path`: trims whitespace; empty or `/` gives `/`; removes trailing `/`s (a result that becomes empty gives `/`); ensures a leading `/`.
* `with_base_path(path)`: uses the process-wide base path (`BASE_PATH`, normalised, read once into a `LazyLock`). With base `/` returns `path` unchanged. Otherwise `""` gives the base, `"/"` gives `"{base}/"`, anything else `"{base}{path}"`.

## `common::wait` (`src/common/wait.rs`)

```rust
#[must_use] pub fn gatehouse_health_url() -> String;
pub async fn wait_for_services(service_name: &str, urls: Vec<&str>);
```

* `gatehouse_health_url()`: `"{GATEHOUSE_URL without trailing '/'}/health/ready"` (with `GATEHOUSE_URL` unset: `"/health/ready"`).
* `wait_for_services`: for each URL in order, prints an `Info` line, then polls with a client (2 s timeout, **certificate verification disabled**) every second until a response has a 2xx status; prints a `Success` line. There is no overall timeout and no error return.
