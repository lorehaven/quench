# Quench

A modular Rust workspace of shared library crates: auth/session verification, an ORM/migration layer, a lightweight HTTP framework and its service bootstrap, a dependency-light HTML/CSS/JS page builder, caching, an authenticated HTTP client, and config loading. Split out of [Forge](https://github.com/lorehaven/forge), which remains the primary consumer alongside other services.

See [docs/](./docs/) for per-crate reference.

## Crates

- [Quench Auth](./docs/quench-auth.md) — relying-party token/session verification
- [Quench Http](./docs/quench-http.md) — lightweight, `poem`-shaped HTTP framework: annotation-discovered routing, dependency injection, plain-async middleware
- [Quench Starter](./docs/quench-starter.md) — service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./docs/quench-web.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./docs/quench-web-components.md) — higher-level UI builders on top of Quench Web
- [Quench DB](./docs/quench-db.md) — ORM/CRUD abstraction plus a migration catalog engine
- [Quench Cache](./docs/quench-cache.md) — shared in-process/Redis caching layer
- [Quench Client](./docs/quench-client.md) — shared authenticated HTTP client wrappers
- [Quench Config](./docs/quench-config.md) — typed config/env loading helper
- [Quench Serde](./docs/quench-serde.md) — JSON/TOML/urlencoded codecs and a dynamic `Value` model, without serde
- [Quench Crypto](./docs/quench-crypto.md) — hashing, HMAC, Ed25519, RSA verify, PBKDF2, Argon2id and JWT
- [Quench Serde YAML](./docs/quench-serde-yaml.md) — the `yaml` feature of Quench Serde: YAML 1.2 on `Value`
- [Quench Core Macros](./docs/quench-core-macros.md) — `#[async_trait]` and `#[derive(Error)]` for Quench Core
- [Quench Serde Macros](./docs/quench-serde-macros.md) — `#[derive(Serialize, Deserialize)]` for Quench Serde
- [Quench Core](./docs/quench-core.md) — dependency-free foundations: time, uuid, base64/hex, errors, env, sharded map, versions, bytes
- [Quench Proto](./docs/quench-proto.md) — HTTP types, an HTTP/1.1 codec for servers and clients, multipart, cookies, WebSocket and SSE
- [Quench QR](./docs/quench-qr.md) — QR code encoder (ISO 18004, versions 1-40) with SVG output, no dependencies
- [Quench PDF](./docs/quench-pdf.md) — PDF text extraction (xref tables and streams, object streams, fonts, encodings)
- [Quench Template](./docs/quench-template.md) — Jinja-compatible templates over quench-serde values (filters, tests, macros, imports)
- [Quench Archive](./docs/quench-archive.md) — tar, zip, deflate, gzip and zstd (decoder and encoder), no dependencies
- [Quench Args](./docs/quench-args.md) — command-line parsing with clap-compatible derives, no clap
- [Quench K8s](./docs/quench-k8s.md) — generic Kubernetes API client (kubeconfig, dynamic resources, discovery, logs, watch)
- [Quench Schema](./docs/quench-schema.md) — JSON Schema validation over quench-serde values
- [Quench Regex](./docs/quench-regex.md) — regular expressions with Unicode classes, no dependencies
- [Quench TZ](./docs/quench-tz.md) — IANA time zones with the database built in
- [Quench TLS](./docs/quench-tls.md) — async glue between tokio streams and rustls, with shared trust-store helpers
- [Quench Log](./docs/quench-log.md) — leveled events, spans, console and rolling-file output
- [Quench Log Macros](./docs/quench-log-macros.md) — `#[instrument]` for Quench Log
- [Quench PG](./docs/quench-pg.md) — PostgreSQL client: wire protocol, SCRAM-SHA-256, TLS, typed rows, pool
- [Quench Redis](./docs/quench-redis.md) — Redis client: RESP2/3, pooled connections, cluster routing
- [Quench Mail](./docs/quench-mail.md) — MIME message builder and async SMTP submission client
- [Quench CLI](./docs/quench-cli.md) — shared terminal UI styling for CLI tools

Most crates have cargo features for the heavier optional parts (TLS, the Postgres backend, the HTTP layer of `quench-auth`, the REPL editor, ...); see [Cargo features](./docs/README.md#cargo-features).

## Build

```bash
cargo build --release
```

## Tests

```bash
cargo test --workspace
```

The Redis-backed cache tests are skipped unless `CACHE_TEST_REDIS_URL` is set;
the cluster ones want `CACHE_TEST_REDIS_CLUSTER_URL` with comma-separated seeds.

## License
MIT
