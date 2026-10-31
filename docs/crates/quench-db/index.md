---
title: quench-db
parent: Crates
nav_order: 16
has_children: true
permalink: /crates/quench-db/
---

`quench-db` (library `quench_db`) is a small ORM and database abstraction layer. It has two independent halves: a `Model`/`Crud` pair that gives plain CRUD access to Postgres or to an in-memory store behind one `Db` enum, and a versioned migration engine (catalog, planner, runner) that installs and evolves database schemas. It replaces `sqlx`-style model code in the estate's services; the Postgres wire client underneath is [quench-pg](../quench-pg/index.md).

* Version: `0.2.14` (`quench-db/Cargo.toml`). Edition `2024` (workspace). No `rust-version` is declared in the manifest or the workspace.
* No derive macros: a model implements `Model` by hand and needs only `quench_serde::Serialize` + `Deserialize` (+ `Clone`, `Debug`, `Send`, `Sync`, `Unpin`).
* No `unsafe` in the crate.

## Install

```toml
[dependencies]
quench-db = "0.2"
```

Without the Postgres side (in-memory backend, catalog and planner only):

```toml
[dependencies]
quench-db = { version = "0.2", default-features = false }
```

## Features

| Feature | Default | Adds |
|---|---|---|
| `postgres` | yes | `dep:quench-pg`; `Db::Postgres`, `Repository::Postgres`, `PostgresDb`, `PostgresRepository`, the whole `runner` module (`MigrationRunner`, reports), `From<quench_pg::Error> for DbError`. |
| `sqlite` | no | `dep:quench-sqlite`; `Db::Sqlite`, `Repository::Sqlite`, `SqliteDb`, `SqliteRepository`, `From<quench_sqlite::Error> for DbError`; `Db::connect("sqlite:path")`. See [the SQLite backend](api-backends.md#backendssqlite-feature-sqlite). |
| `tls` | yes | `quench-pg?/tls`: `sslmode=require` and `verify-full` work. No effect without `postgres`. |

Without `postgres`, `Db::connect("")` returns an in-memory database and `Db::connect(url)` with a non-empty URL returns `DbError::ConnectionError("cannot connect to {url}: quench-db was built without its `postgres` feature")`.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `async_trait`, the `Error` derive, `env::var`, `version::{Version, VersionReq}`, `hex`. |
| `quench-serde` | `Serialize`/`Deserialize`/`DeserializeOwned`, `Value`, `to_value`/`from_value`, TOML parsing of migration files. |
| `quench-crypto` | SHA-256 for plan checksums and the advisory-lock key. |
| `quench-log` | `info!`/`warn!` while migrating. |
| `quench-pg` (optional) | Postgres client and pool. |
| dev: `quench-test`, `tokio` | The test harness (see [testing](testing.md)). |

Third-party packages in `cargo tree -e normal` (offline, workspace lockfile): 10 package versions without default features (`cfg-if`, `getrandom` 0.2 and 0.4, `libc`, `proc-macro2`, `quote`, `ring`, `syn`, `unicode-ident`, `untrusted`); 26 with the default features (adds `tokio`, `rustls`, `rustls-webpki`, `rustls-pki-types`, `bytes`, `mio`, `socket2`, `subtle`, `log`, `once_cell`, and others pulled by `tokio`/`rustls`).

## Where to go next

* [Usage](usage.md): worked examples (models, repositories, migrations).
* API reference: [core types](api.md), [migration files](api-migrations.md), [catalog](api-catalog.md), [plan](api-plan.md), [runner](api-runner.md), [backends](api-backends.md).
* [Configuration](configuration.md): `DB_POOL_MAX_SIZE`, `DB_MIGRATION_TABLE`, runner options.
* [Errors](errors.md), [Internals](internals.md), [Testing](testing.md).

## Minimal example

```rust
use quench_db::prelude::*;
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
}

impl Model for User {
    fn table_name() -> String { "users".to_string() }
    fn columns() -> Vec<&'static str> { vec!["id", "name"] }
}

async fn demo() -> Result<(), DbError> {
    // An empty URL gives the in-memory backend; "postgres://..." gives Postgres.
    let db = Db::connect("").await?;
    let users = db.repository::<User>();
    users.create(&User { id: "1".into(), name: "ann".into() }).await?;
    assert_eq!(users.read("1").await?.unwrap().name, "ann");
    Ok(())
}
```

This is the shape exercised by `tests/unit/lib_tests.rs` (`repository_dispatches_crud_to_the_in_memory_backend`).
