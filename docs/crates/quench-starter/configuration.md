---
title: Configuration
parent: quench-starter
grand_parent: Crates
nav_order: 5
---

All variables are read through `quench_core::env`. All are optional unless stated.

| Variable | Default | Read in | Meaning |
|---|---|---|---|
| `BASE_PATH` | `/` | `serve`; `common::routes` (`with_base_path`, `ui_path`) | Path prefix all routes are mounted under. `with_base_path` caches the normalised value for the process lifetime. |
| `SERVER_ADDR` | `0.0.0.0:443` | `serve_app` | Listen address (HTTPS when TLS loads, else plain HTTP). Must parse as a socket address, otherwise panic `SERVER_ADDR must be a valid socket address`. |
| `SERVER_HTTP_REDIRECT_ADDR` | `0.0.0.0:80` | `serve_app` | Address of the HTTP-to-HTTPS redirect server. Parsed even when TLS is off (panic if invalid). |
| `SERVER_CERT_PATH` | `cert.pem` | `serve_app` | PEM certificate chain. |
| `SERVER_KEY_PATH` | `key.pem` | `serve_app` | PEM private key. When both load (`quench_http::load_tls`), HTTPS plus redirect server run; otherwise plain HTTP on `SERVER_ADDR`. |
| `SERVICE_NAME` | `service` | `serve` | Name for startup lines; `serve` passes `"{SERVICE_NAME}-service"` to `serve_app`. |
| `DATABASE_URL`, `POSTGRES_URL` | empty | `DbWrapper::init_env` | First non-empty of the two (`get_any`). |
| `ALLOW_IN_MEMORY_DB` | `false` | `DbWrapper::init` | Must be true to accept an empty database URL. |
| `GATEHOUSE_URL` | empty | `gatehouse_health_url` | Base URL; `/health/ready` is appended after trailing `/` is trimmed. |
| `LOG_SKIP_PREFIXES` | empty | `FilteredLogger::default` | Comma-separated path prefixes not logged on success. |
| `RUST_LOG` | `info` (as reported by `logging::init`) | `quench_log::init` | Verbosity; other `LOG_*` variables are read by `quench-log`. |
| `CARGO_BIN_NAME` | unset | `logging::init` | When set, `docker/<name>/.env` is also loaded. |

## `.env` loading (`logging::init`)

1. The first `.env` found in the working directory or the nearest ancestor is loaded.
2. If `CARGO_BIN_NAME` is set and `docker/<CARGO_BIN_NAME>/.env` exists (relative to the working directory), it is loaded too.
3. A variable already present in the process environment is never overwritten.

Syntax: `KEY=VALUE` per line; blank lines and `#` lines ignored; optional `export ` prefix; a value starting with `"` or `'` ends at the next matching quote (an unterminated quote takes the rest); an unquoted value ends at ` #`; lines without `=` or with an empty key are ignored. No variable expansion.
