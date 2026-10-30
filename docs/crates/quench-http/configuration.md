---
title: Configuration
parent: quench-http
grand_parent: Crates
nav_order: 7
---

`quench-http` reads **no environment variables and no files** (the source contains no `env::var`). Everything is set in code.

| Setting | Where | Default | Notes |
|---|---|---|---|
| Body cap for `Json`, `Form`, `Bytes`, `Multipart` | `ContainerBuilder::provide(BodyLimit(n))` | `DEFAULT_BODY_LIMIT` = 2 097 152 bytes | per call site: `Limited<_, N>` |
| Route prefix | `mount(prefix, app)` | none | typically from a `BASE_PATH` env var read by the application (`quench-starter` does this; not this crate) |
| Bind address | `serve_http`/`serve_https` `addr`, or a listener given to `*_on` | - | |
| TLS | `load_tls(cert, key)` returns `Option<rustls::ServerConfig>` with ALPN `http/1.1` | - | feature `tls` |
| WebSocket size limits | `WebSocketUpgrade::config(Config)` / `ws::upgrade_with` | 16 MiB message, 16 MiB frame | |
| Graceful shutdown wait | constant `DEFAULT_GRACEFUL_TIMEOUT` | 25 s | not configurable |
| Header read timeout | constant `DEFAULT_HEADER_READ_TIMEOUT` | 30 s | not configurable |
| Shutdown trigger | `*_on` `shutdown` future | `SIGINT`/`SIGTERM` for `serve_http`/`serve_https` | |

The remaining connection settings come from `quench_proto::h1::server::ServerConfig::default()` with only `header_read_timeout` overridden (to the 30 s above): head limits `max_head_bytes` 64 KiB, `max_headers` 100, `max_uri_bytes` 8 KiB; `idle_timeout` 75 s; `write_timeout` 60 s; `drain_limit` 1 MiB; `drain_timeout` 2 s (values read from `quench-proto/src/h1/{server,parse}.rs`). They cannot be changed through `quench-http`.

Cargo feature: `tls` (default), see the [overview](index.md#features).
