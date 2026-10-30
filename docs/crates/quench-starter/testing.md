---
title: Testing
parent: quench-starter
grand_parent: Crates
nav_order: 8
---

Four integration test binaries, all `harness = false` with the `quench_test` harness: `boot` (`tests/boot.rs`), `http_health`, `http_ui_redirects`, `unit` (`tests/unit.rs`, with modules `common_db_tests`, `common_format_tests`, `common_wait_tests`, `http_pieces_tests`, `http_routers_ui_common_css_tests`, `http_routers_ui_pages_home_tests`, `metrics_tests`, `misc_tests`, `resilience_tests`). In-source tests exist in `src/http/mod.rs` (`RootOrMounted`, 3 tests) and `src/logging.rs` (`.env` parsing and loading, 3 tests). 46 `#[quench_test::test]` functions in total.

```bash
cargo test -p quench-starter
```

* `boot.rs` boots real servers: `serve_app` plain and with TLS plus the HTTP redirect (using the certificates in `quench-tls/tests/fixtures`), and `serve`. The environment is process-global, so these tests take turns on an `ENV_LOCK` mutex.
* `http_health.rs`, `http_ui_redirects.rs` exercise `discover_and_mount` for the health routes and the UI redirects.

## Testing a service built on the crate

Build the app with `discover_and_mount("/")` (or your base path) and call it as an `Endpoint` with a `quench_http` `Request` and a container with `HealthState`. For database tests set `ALLOW_IN_MEMORY_DB=true` and an empty URL.
