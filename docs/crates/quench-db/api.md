---
title: API - core types
parent: quench-db
grand_parent: Crates
nav_order: 2
---

Items defined in `src/lib.rs`, `src/error.rs` and `src/prelude.rs`. Other modules: [migration files](api-migrations.md), [catalog](api-catalog.md), [plan](api-plan.md), [runner](api-runner.md), [backends](api-backends.md).

## Crate root: modules and re-exports

```rust
pub mod backends;
pub mod catalog;
pub mod error;
pub mod migrations;
pub mod plan;
pub mod prelude;
#[cfg(feature = "postgres")] pub mod runner;

pub use backends::in_memory::{InMemoryDb, InMemoryRepository};
#[cfg(feature = "postgres")] pub use backends::postgres::{PostgresDb, PostgresRepository};
pub use catalog::{Catalog, CatalogError, CatalogModule, ModuleManifest, ModuleScope, Requirement};
pub use error::DbError;
pub use migrations::{ChangeSet, ColumnDef, Migration, MigrationFile, MigrationLoader};
pub use plan::{InstallRequest, MigrationPlan, PlanError, PlannedMigration, ResolvedModule};
#[cfg(feature = "postgres")]
pub use runner::{ApplyReport, MigrationOutcome, MigrationResult, MigrationRunner, ResetReport};
```

Not re-exported at the root: `catalog::MODULE_MANIFEST`, `catalog::migration_since`, `migrations::render`, `migrations::RenderError`, the `runner` constants and functions (reach them by module path).

## `prelude`

`quench_db::prelude` re-exports: `PostgresDb`, `PostgresRepository` (feature `postgres`); `Catalog`, `CatalogError`, `CatalogModule`, `ModuleManifest`, `ModuleScope`, `Requirement`; `DbError`; `ChangeSet`, `ColumnDef`, `Migration`, `MigrationFile`, `MigrationLoader`; `InstallRequest`, `MigrationPlan`, `PlanError`, `PlannedMigration`, `ResolvedModule`; `ApplyReport`, `MigrationOutcome`, `MigrationResult`, `MigrationRunner`, `ResetReport` (feature `postgres`); and from the root `Crud`, `Database`, `Db`, `Model`, `Repository`. It does **not** include `InMemoryDb`, `InMemoryRepository`, `RenderError` or `MODULE_MANIFEST`.

## `trait Database`

```rust
#[async_trait]
pub trait Database: Send + Sync + Debug {
    async fn execute(&self, query: &str) -> Result<(), DbError>;
    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError>;
}
```

A connection-level handle. Implemented by `Db`, `PostgresDb`, `SqliteDb` (feature `sqlite`), `InMemoryDb`.

* `execute`: run SQL, ignoring any rows. Postgres: simple protocol, so a script with several statements is accepted. In-memory: always `Ok(())`, the query is ignored.
* `migrate`: apply `Migration`s not yet recorded (the simple, ledger-table path; not the catalog runner). Postgres: see [backends](api-backends.md#database-for-postgresdb). In-memory: always `Ok(())`.

## `trait Model`

```rust
#[async_trait]
pub trait Model: Serialize + DeserializeOwned + Send + Sync + Debug + Unpin + Clone {
    fn table_name() -> String;
    fn columns() -> Vec<&'static str>;
    #[must_use]
    fn primary_key_name() -> String { "id".to_string() }
}
```

Describes how a type maps to a table. `Serialize`/`DeserializeOwned` are `quench_serde`'s. The `#[async_trait]` attribute on the trait has no effect (no async methods).

* `table_name`: the table, interpolated into SQL as written (unquoted, unescaped). May be schema-qualified (`"app.users"`); nothing prevents it.
* `columns`: the column names. Used by `find_by` (allow-list) and `update` (the `SET` list). Not used by `create`, `read`, `list`, `delete`.
* `primary_key_name`: default `"id"`. Used by `read`, `update`, `delete` (Postgres, interpolated unquoted) and by the in-memory backend as the JSON key of the id.

## `trait Crud<T: Model>`

```rust
#[async_trait]
pub trait Crud<T: Model>: Send + Sync {
    async fn create(&self, model: &T) -> Result<T, DbError>;
    async fn read(&self, id: &str) -> Result<Option<T>, DbError>;
    async fn update(&self, model: &T) -> Result<T, DbError>;
    async fn delete(&self, id: &str) -> Result<(), DbError>;
    async fn list(&self) -> Result<Vec<T>, DbError>;
    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError>;
}
```

| Method | Returns | Notes |
|---|---|---|
| `create` | the stored row | Inserts. Postgres returns the row as read back with `RETURNING *`; in-memory returns a clone of the argument. |
| `read` | `Some(row)` or `None` | A missing row is `None`, not an error. |
| `update` | the stored row | Postgres: a missing row is `DbError::NotFound`. In-memory: inserts or overwrites (never fails for a missing row). |
| `delete` | `()` | A missing row is not an error on either backend. |
| `list` | all rows | Order unspecified. No paging. |
| `find_by` | rows where `column = value` | `column` must be one of `T::columns()`, else `DbError::Unknown("unknown column '{column}' for table {table}")`. The value is bound as a parameter. |

Implemented by `Repository<T>`, `PostgresRepository<T>`, `InMemoryRepository<T>`.

## `enum Db`

```rust
#[derive(Clone, Debug)]
pub enum Db {
    #[cfg(feature = "postgres")] Postgres(PostgresDb),
    InMemory(InMemoryDb),
}
```

* `pub async fn connect(url: &str) -> Result<Self, DbError>`: an empty `url` returns `Db::InMemory(InMemoryDb::new())` (fresh, empty). A URL starting with `sqlite:` (`sqlite:path` or `sqlite://path`) returns `Db::Sqlite(SqliteDb::open(url)?)` with feature `sqlite`. Any other non-empty URL returns `Db::Postgres(PostgresDb::new(url).await?)` with feature `postgres` (the URL is parsed by `quench_pg::Config::from_url`; any scheme other than `postgres://`/`postgresql://` is `ConnectionError`). Without the feature: `DbError::ConnectionError("cannot connect to {url}: quench-db was built without its `postgres` feature")`.
* `#[must_use] pub fn repository<T: Model>(&self) -> Repository<T>`: a repository dispatching to this backend. Cheap (clones the handle).
* `impl Database for Db`: forwards `execute` and `migrate` to the variant.

## `enum Repository<T: Model>`

```rust
pub enum Repository<T: Model> {
    #[cfg(feature = "postgres")] Postgres(PostgresRepository<T>),
    InMemory(InMemoryRepository<T>),
}
```

`impl<T: Model> Debug` (prints `Postgres(..)`/`InMemory(..)`), and `impl<T: Model> Crud<T>` forwarding every method to the variant. No `Clone` implementation.

## `enum DbError`

```rust
#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database connection error: {0}")]      ConnectionError(String),
    #[error("Query execution error: {0}")]          QueryError(String),
    #[error("Serialization error: {0}")]            SerializationError(String),
    #[error("Record not found")]                    NotFound,
    #[error("Unknown database error: {0}")]         Unknown(String),
}
```

Conversions: `From<quench_pg::Error>` (feature `postgres`) and `From<quench_serde::Error>` (always, to `SerializationError`). Mapping and meaning: [errors](errors.md).

## Hidden/internal

`pub(crate) fn checked_column<T: Model>(column: &str) -> Result<&'static str, DbError>` is not public: it returns the matching entry of `T::columns()` or the `Unknown` error above.
