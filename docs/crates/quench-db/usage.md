---
title: Usage
parent: quench-db
grand_parent: Crates
nav_order: 1
---

Task-oriented examples, from the common to the advanced. Every name is real; examples follow `tests/` where a test exists.

## 1. Define a model

```rust
use quench_db::Model;
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Widget {
    id: String,
    name: String,
}

impl Model for Widget {
    fn table_name() -> String { "widgets".to_string() }
    fn columns() -> Vec<&'static str> { vec!["id", "name"] }
    // primary_key_name() defaults to "id"
}
```

`columns()` must list every column that `find_by` may filter on, and (for `update`) every column that is written. Pitfalls: the table name and the primary-key name are pasted into SQL unquoted and unchecked (they come from your code, not from input); `Model: Clone` is required, so a model without `Clone` does not compile.

## 2. Connect and get a repository

```rust
let db = quench_db::Db::connect(&url).await?;   // "" -> in-memory
let widgets = db.repository::<Widget>();         // Repository<Widget>
```

`Db` is `Clone`; clones share the pool (Postgres) or the tables (in-memory). A `Repository<T>` implements `Crud<T>`; import the trait (`use quench_db::Crud;` or the prelude).

## 3. CRUD

```rust
use quench_db::Crud;
let w = Widget { id: "1".into(), name: "sprocket".into() };
let created = widgets.create(&w).await?;        // returns the stored row
let read    = widgets.read("1").await?;         // Option<Widget>
let updated = widgets.update(&Widget { name: "gear".into(), ..w.clone() }).await?;
let all     = widgets.list().await?;            // order unspecified
let some    = widgets.find_by("name", "gear").await?;
widgets.delete("1").await?;                     // Ok even if the row was absent
```

Effects on Postgres are described in [backends](api-backends.md). Pitfalls: the two backends differ in edge cases (duplicate `create`, `update` of a missing row, `find_by` on non-text columns); see [internals](internals.md#backend-differences).

## 4. Numeric primary keys

`read`, `delete` take `&str`. For an integer key pass its decimal text; the Postgres backend sends it as a text parameter that the server types from the column. The model field can be any integer type.

```rust
// id: i64 in the struct, "id BIGINT PRIMARY KEY" in the table
let row = repo.read("42").await?;
```

The in-memory backend stores numeric keys by their decimal text (`i64` only; see [errors](errors.md)).

## 5. Run raw SQL / simple migrations

```rust
use quench_db::{ChangeSet, Database, Migration};

db.execute("CREATE TABLE IF NOT EXISTS widgets (id TEXT PRIMARY KEY, name TEXT NOT NULL)").await?;

db.migrate(vec![Migration {
    id: "0001-widgets".into(),
    author: "me".into(),
    since: None,
    changes: vec![ChangeSet::Sql("CREATE TABLE widgets (id TEXT PRIMARY KEY)".into())],
}]).await?;
```

On Postgres `execute` uses the simple protocol, so a `;`-separated script is fine. `migrate` records applied ids in the table named by `DB_MIGRATION_TABLE` (default `quench_migrations`) and skips ids already recorded; each migration runs in its own transaction. `${var}` placeholders are **not** substituted on this path. On the in-memory backend both calls are no-ops.

## 6. Module catalog: files

A catalog is a directory of modules:

```text
migrations/
  auth/
    module.toml
    0001-users.toml
  app/
    module.toml
    0001-tables.toml
```

`migrations/auth/module.toml`:

```toml
name = "auth"
version = "0.1.0"
```

`migrations/app/module.toml`:

```toml
name = "app"
version = "0.2.0"
default_schema = "app"
requires = ["auth@^0.1"]

[variables]
retention = "30"
```

`migrations/app/0001-tables.toml`:

```toml
[[migrations]]
id = "0001-tables"
author = "me"
since = "0.1.0"

[[migrations.changes]]
createSchema = { name = "${schema}" }

[[migrations.changes]]
sql = "CREATE TABLE ${schema}.early (id TEXT PRIMARY KEY, days INT DEFAULT ${retention})"
```

The shapes match `tests/catalog_plan.rs`. Reference: [migration files](api-migrations.md), [catalog](api-catalog.md).

## 7. Plan and apply

```rust
use quench_db::prelude::*;

let catalog = Catalog::load("migrations")?;                 // CatalogError
let plan = MigrationPlan::resolve(
    &catalog,
    &[InstallRequest::parse("app@0.2.0:app_test")?],        // PlanError
)?;
let db = PostgresDb::new(&url).await?;
let report = MigrationRunner::new().apply(&db, &plan).await?;
println!("{} applied", report.count(MigrationOutcome::Applied));
```

Dependencies of `app` (here `auth`) are added automatically and installed into the same schema unless the `requires` entry names another. Run it twice: the second report has every migration `Skipped`.

## 8. Dry run, drift, reset

```rust
let pending = MigrationRunner::new().dry_run(true).apply(&db, &plan).await?;
for r in pending.with_outcome(MigrationOutcome::Pending) { println!("{}", r.id); }

// changed SQL after it was applied: an error ...
MigrationRunner::new().apply(&db, &plan).await?;
// ... unless you accept it (logs a warning)
MigrationRunner::new().allow_drift(true).apply(&db, &plan).await?;

// development only: drop the plan's schemas and forget the ledger rows
let gone = MigrationRunner::new().reset(&db, &plan).await?;
println!("dropped {:?}", gone.schemas);
```

Pitfall: `reset` ignores `dry_run` (see [runner](api-runner.md#reset)).

## 9. Write tests without a database

Use `Db::connect("")` (or `InMemoryDb::new()` with `InMemoryRepository::new`) in unit tests; for catalog logic write a temporary directory with `module.toml` files as `tests/catalog_plan.rs` does (`quench_core::tmp::TempDir`).
