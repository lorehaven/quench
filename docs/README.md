# Quench Documentation

Quench is a modular Rust workspace of shared library crates split out of [Forge](https://github.com/lorehaven/forge), which remains the primary consumer alongside other services. Jump straight to a crate's page below.

## Layout

```text
docs/
├── README.md                 # this file
├── quench-auth.md
├── quench-cache.md
├── quench-cli.md
├── quench-client.md
├── quench-config.md
├── quench-db.md
├── quench-starter.md
├── quench-web.md
├── quench-web-components.md
└── examples/                 # examples/* runnable references
    ├── basic.md
    └── db_example.md
```

Each page lives at the same path as the crate it documents (e.g. `quench-auth` → `docs/quench-auth.md`), so the doc to update is always predictable from the code you're touching. Every crate's own `README.md` is a short pointer into its page here — this tree is the actual documentation: descriptions, features, configuration, and API surface.

## Crates

- [Quench Auth](./quench-auth.md) — relying-party token/session verification
- [Quench Starter](./quench-starter.md) — Actix service bootstrap (TLS, base-path scoping, health, DB wiring)
- [Quench Web](./quench-web.md) — dependency-light server-rendered HTML/CSS/JS page builder
- [Quench Web Components](./quench-web-components.md) — higher-level UI builders on top of Quench Web
- [Quench DB](./quench-db.md) — ORM/CRUD abstraction plus a migration catalog engine
- [Quench Cache](./quench-cache.md) — shared in-process/Redis caching layer
- [Quench Client](./quench-client.md) — shared authenticated HTTP client wrappers
- [Quench Config](./quench-config.md) — typed config/env loading helper
- [Quench CLI](./quench-cli.md) — shared terminal UI styling for CLI tools

The `docker/*` services and several `cli/*` tools in the sibling [Forge](https://github.com/lorehaven/forge) repository depend on these crates, pulled in from the `ennor` cargo registry like any other dependency — see that repo's own docs for how they're used in practice.

## Examples

- [Example: Basic](./examples/basic.md) — smallest possible Quench web app, a smoke test for the UI framework
- [Example: Db](./examples/db_example.md) — `quench-db` migration runner and CRUD repository against Postgres

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

## Project Structure

```text
.
├── quench-auth/            # relying-party token/session verification
├── quench-cache/           # in-process/Redis caching layer
├── quench-cli/             # shared terminal UI styling for CLI tools
├── quench-client/          # authenticated HTTP client wrappers
├── quench-config/          # typed config/env loading
├── quench-db/              # ORM/CRUD abstraction plus migrations
├── quench-starter/         # Actix service bootstrap
├── quench-web/             # server-rendered HTML/CSS/JS page builder
├── quench-web-components/  # UI components built on quench-web
├── examples/               # runnable references: basic, db_example
└── docs/                   # this documentation tree
```
