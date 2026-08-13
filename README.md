# Quench

A modular Rust workspace of shared library crates: auth/session verification, an ORM/migration layer, an actix service bootstrap, a dependency-light HTML/CSS/JS page builder, caching, an authenticated HTTP client, and config loading. Split out of [Forge](https://github.com/lorehaven/forge), which remains the primary consumer alongside other services.

See [docs/](./docs/) for per-crate reference.

## Crates

- [Quench Auth](./docs/quench-auth.md) — relying-party token/session verification
- [Quench Starter](./docs/quench-starter.md) — Actix service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./docs/quench-web.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./docs/quench-web-components.md) — higher-level UI builders on top of Quench Web
- [Quench DB](./docs/quench-db.md) — ORM/CRUD abstraction plus a migration catalog engine
- [Quench Cache](./docs/quench-cache.md) — shared in-process/Redis caching layer
- [Quench Client](./docs/quench-client.md) — shared authenticated HTTP client wrappers
- [Quench Config](./docs/quench-config.md) — typed config/env loading helper
- [Quench CLI](./docs/quench-cli.md) — shared terminal UI styling for CLI tools

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
