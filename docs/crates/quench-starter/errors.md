---
title: Errors
parent: quench-starter
grand_parent: Crates
nav_order: 6
---

The crate has no error enum. Failures are of three kinds.

## Panics at startup

| Where | Condition | Message |
|---|---|---|
| `DbWrapper::init` / `init_env` | Empty (after trim) URL and `ALLOW_IN_MEMORY_DB` not true | `no database configured: set DATABASE_URL (or POSTGRES_URL). Set ALLOW_IN_MEMORY_DB=true only for tests, where losing every write on restart is the intent.` |
| `DbWrapper::init` | `Db::connect` fails | `configured database connection failed: {e}` |
| `serve` | DI graph fails | `dependency graph failed to resolve: {e}` |
| `serve_app` | Bad address variable | `SERVER_ADDR must be a valid socket address` / `SERVER_HTTP_REDIRECT_ADDR must be a valid socket address` |
| `RequestMetrics::record_error`, `reset`; `CircuitBreaker` methods | A poisoned mutex (`unwrap`) | standard `PoisonError` panic |

## `std::io::Error`

`serve` and `serve_app` return `std::io::Result<()>` from the servers (`serve_http`, `serve_https`, `try_join!`).

## HTTP error bodies

* `http::domain::api_error`: `{"error": "<message>"}` (see [API: http](api-http.md)). If JSON serialization fails the body is the plain message text.
* `http::domain::error`: `{"errors":[{"code":"UNAUTHORIZED|DENIED|UNSUPPORTED|...","message":"...","detail":{}}]}`.
* `ui::serve_assets`: `400` for an unsafe or empty path, `404` for a file that cannot be read.
* `/health/live`, `/health/ready`: `503` with `{"status":"not_live"}` / `{"status":"not_ready"}`.
