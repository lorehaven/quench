# quench-starter
Server bootstrap and utilities for Quench HTTP services.

## What it is
`quench-starter` is the server bootstrap crate shared by the Forge HTTP services. It wraps what every service repeats: HTTPS with an HTTP-to-HTTPS redirect server (or plain HTTP), base-path scoping, health and readiness routes, request logging, correlation ids, security headers, database bootstrap via `quench-db`, and logging initialisation, behind one entry point, `http::serve`.

It also carries a few helpers: waiting for other services, retries with backoff, a circuit breaker and request metrics. It builds on `quench-http`, `quench-auth`, `quench-db`, `quench-log` and `quench-client`.

## Use cases
- Start a service with health and readiness routes in a few lines.
- Serve under a configurable `BASE_PATH`.
- Hold readiness at 503 until dependencies are up.
- Return JSON errors from handlers with `ApiError`.
- Call upstream services with `retry_with_backoff` and a `CircuitBreaker`.

## Example
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

`serve` reads `BASE_PATH`, connects the database unless one is passed, marks the service ready when `init` completes, discovers routes and serves HTTPS (if the certificate and key load) or plain HTTP.

```rust
use quench_starter::prelude::{ApiError, normalize_base_path};
use quench_http::prelude::http::StatusCode;

assert_eq!(normalize_base_path(" api/ "), "/api");
let err = ApiError::new(StatusCode::NOT_FOUND, "no such item"); // {"error":"no such item"}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-starter/
