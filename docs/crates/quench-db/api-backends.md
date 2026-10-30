---
title: API - backends
parent: quench-db
grand_parent: Crates
nav_order: 3
---

Module `quench_db::backends` (`src/backends/mod.rs`): `pub mod in_memory;` and, with feature `postgres`, `pub mod postgres;`.

## `backends::postgres` (feature `postgres`)

### `struct PostgresDb`

```rust
#[derive(Clone)]
pub struct PostgresDb { pool: Pool }   // quench_pg::Pool

pub async fn new(url: &str) -> Result<Self, DbError>
#[must_use] pub const fn pool(&self) -> &Pool
```

`new` parses `url` with `quench_pg::Config::from_url`, reads `DB_POOL_MAX_SIZE` (default 5; unparsable text also gives 5; see [configuration](configuration.md)), and calls `Pool::connect_with(config, PoolOptions { max_size, ..Default })`, which opens one connection immediately. Errors map through `From<quench_pg::Error>` (a bad URL, unreachable host or rejected credentials are `ConnectionError`). `Debug` prints `PostgresDb` only (no URL, no password). `pool()` exposes the pool for raw queries.

### `impl Database for PostgresDb`

* `execute(query)`: `pool.batch_execute(query)`, the simple protocol, so multi-statement scripts work. Rows are discarded.
* `migrate(migrations)`: creates the ledger table if missing

  ```sql
  CREATE TABLE IF NOT EXISTS {table} (
      id TEXT PRIMARY KEY,
      author TEXT NOT NULL,
      applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
  )
  ```

  with `{table}` = `DB_MIGRATION_TABLE` (default `quench_migrations`, read at every call, not validated). For each migration in order: `SELECT EXISTS(SELECT 1 FROM {table} WHERE id = $1)`; if absent, logs `info!("Applying migration: {id}")`, opens a transaction on a pooled connection, runs each `ChangeSet::to_sql()` (no variable rendering, `since` is ignored), inserts `(id, author)`, commits. A failure rolls that migration back and returns the error; earlier migrations stay applied. No advisory lock is taken.

### `struct PostgresRepository<T: Model>`

```rust
pub struct PostgresRepository<T: Model> { db: PostgresDb, _marker: PhantomData<T> }
#[must_use] pub const fn new(db: PostgresDb) -> Self
```

`Debug` prints `PostgresRepository { .. }`. `impl<T: Model> Crud<T>` generates this SQL (`{t}` = `T::table_name()`, `{pk}` = `T::primary_key_name()`, no quoting). Rows become models with `quench_pg::Row::deserialize::<T>()` (column name to field name).

| Method | SQL | Parameters |
|---|---|---|
| `create` | `INSERT INTO {t} SELECT * FROM jsonb_populate_record(NULL::{t}, $1) RETURNING *` | `$1` = the model as a JSON `Value` |
| `read` | `SELECT * FROM {t} WHERE {pk} = $1` | `$1` = id text; `query_opt` |
| `update` | `UPDATE {t} SET ({cols}) = (SELECT {cols} FROM jsonb_populate_record(NULL::{t}, $1)) WHERE {pk} = $2 RETURNING *` | `$1` = JSON, `$2` = the model's key as text; `query_one` |
| `delete` | `DELETE FROM {t} WHERE {pk} = $1` | `$1` = id text |
| `list` | `SELECT * FROM {t}` | none |
| `find_by` | `SELECT * FROM {t} WHERE {column} = $1` | `$1` = value text; `column` allow-listed against `T::columns()` |

`{cols}` is `T::columns().join(", ")`. Details:

* `update` reads the key from the serialised model under `primary_key_name()`; string, signed or unsigned integer keys are accepted (rendered as text), anything else is `DbError::Unknown("Primary key '{pk}' is not a string or number")`; a model without that field is `Unknown("Primary key '{pk}' not found in model")`. An update that matches no row fails with `DbError::NotFound` (`RowCount { got: 0 }`).
* `create` goes through `jsonb_populate_record`: model fields absent from the JSON become NULL in the insert, not the column default.
* Because parameters are sent untyped (text), the server types `$1` in `read`/`delete`/`find_by` from the column; an integer key works from its decimal text.
* Field and column type mapping is `quench_pg`'s (see [quench-pg types](../quench-pg/api-types.md)): `Time` fields are `timestamptz`/`timestamp` columns (RFC 3339 strings), `Uuid`, `Value` for `json`/`jsonb`, `Vec<_>` for arrays, `Option<_>` for nullable.

## `backends::in_memory`

### `struct InMemoryDb`

```rust
#[derive(Clone, Default)]
pub struct InMemoryDb { tables: Arc<RwLock<HashMap<String, Arc<RwLock<HashMap<String, Value>>>>>> }
#[must_use] pub fn new() -> Self
```

A map of table name to (id text to JSON `Value`). Clones share the same tables; separate `new()` values are isolated. Tables are created on first use. `Debug` prints `InMemoryDb`. `impl Database`: `execute` and `migrate` ignore their arguments and return `Ok(())`. Lock poisoning panics (`unwrap`).

### `struct InMemoryRepository<T: Model>`

```rust
pub struct InMemoryRepository<T: Model> { db: InMemoryDb, _marker: PhantomData<T> }
#[must_use] pub const fn new(db: InMemoryDb) -> Self
```

`Debug` prints `InMemoryRepository { .. }`. `impl Crud<T>`:

* `create`: serialises the model with `quench_serde::to_value`, takes the id from field `primary_key_name()` (missing: `Unknown("Missing primary key")`; neither string nor `i64`: `Unknown("Primary key must be string or number")`), stores it under that text, replacing any existing row, returns `model.clone()`.
* `read`: deserialises the stored value (`SerializationError` on failure).
* `update`: identical to `create` (upsert).
* `delete`: removes the key; absent is fine.
* `list`: every stored row, in `HashMap` order.
* `find_by`: allow-list check like Postgres, then rows whose field `column` is a JSON string equal to `value`. Non-string fields never match.
