---
title: Usage
parent: quench-starter
grand_parent: Crates
nav_order: 1
---

## 1. The common case: `serve`

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    quench_starter::logging::init();
    quench_starter::http::serve(None, async {
        // wait for dependencies, warm caches, ...
    })
    .await
}
```

`serve` reads `BASE_PATH`, connects the database (`DbWrapper::init_env`) unless one is passed, spawns `init` and marks the service ready when it completes, builds the DI container with `Db` and `HealthState`, discovers routes and serves HTTPS (if the certificate and key load) or plain HTTP. `/health` answers at once, `/health/ready` answers 503 until `init` finishes.

## 2. A service with auth middleware: `discover_and_mount` and `serve_app`

`serve` cannot add providers or middleware; compose the pieces yourself:

```rust
use std::sync::Arc;
use quench_http::prelude::*;

let base_path = quench_starter::prelude::normalize_base_path(&quench_core::env::get_or("BASE_PATH", "/"));
let app = quench_starter::http::discover_and_mount(base_path);
let app = wrap(app, Auth::new(config));       // Auth (quench_auth) must be outermost
let container = Arc::new(ContainerBuilder::new().provide(extra).build().await?);
quench_starter::http::serve_app("my-service", app, container).await?;
```

Include `HealthState` in your container if you want `/health/live` and `/health/ready` to work (`Inject<HealthState>`).

## 3. Readiness while dependencies come up

```rust
use quench_starter::prelude::{HealthState, wait_for_services, gatehouse_health_url};

let health = HealthState::live();
health.spawn_ready_when(async {
    wait_for_services("my-service", vec![&gatehouse_health_url()]).await;
});
```

`wait_for_services` polls each URL once a second until it answers 2xx, with no upper limit.

## 4. Base paths

```rust
use quench_starter::prelude::{normalize_base_path, with_base_path};
assert_eq!(normalize_base_path(" api/ "), "/api");
// with BASE_PATH=/svc : with_base_path("/x") == "/svc/x"
```

`with_base_path` reads `BASE_PATH` once per process.

## 5. JSON errors from handlers

```rust
use quench_starter::prelude::{ApiError, json_error};
use quench_http::prelude::http::StatusCode;

async fn handler() -> Result<Response, ApiError> {
    Err(ApiError::new(StatusCode::NOT_FOUND, "no such item"))   // {"error":"no such item"}
}
```

A 5xx `ApiError` is logged when converted.

## 6. Retries and a circuit breaker

```rust
use quench_starter::{RetryConfig, retry_with_backoff, CircuitBreaker};

let breaker = CircuitBreaker::default();            // 5 failures, 3 successes, 60 s
if breaker.is_available() {
    match retry_with_backoff(|| call_service(), RetryConfig::default()).await {
        Ok(v) => { breaker.call_succeeded(); /* ... */ }
        Err(_) => breaker.call_failed(),
    }
}
```

## 7. Request metrics

```rust
use quench_starter::{RequestMetrics, TimedBlock};

let metrics = RequestMetrics::new();
let t = TimedBlock::new();
metrics.record_success(t.elapsed_ms());
let text = metrics.snapshot().to_prometheus("upstream");
```

## 8. UI login redirects

```rust
use quench_starter::prelude::routers::ui::{ui_login_redirect_for, ui_path};
// htmx requests (HX-Request header) get 200 + HX-Redirect; others get 302 Location: <base>/ui/login
```
