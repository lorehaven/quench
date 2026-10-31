# quench-db
A simple ORM and database abstraction layer.

## What it is
`quench-db` has two independent halves: a `Model`/`Crud` pair that gives plain CRUD access to Postgres or to an in-memory store behind one `Db` enum, and a versioned migration engine (catalog, planner, runner) that installs and evolves database schemas. It replaces `sqlx`-style model code in the estate's services.

There are no derive macros: a model implements `Model` by hand and needs only `quench_serde::Serialize` and `Deserialize`. The Postgres wire client underneath is `quench-pg`; the crate also uses `quench-core`, `quench-serde`, `quench-crypto` and `quench-log`.

## Use cases
- CRUD on a table through `Repository<T>`, with `find_by` on a column.
- Run service tests against the in-memory backend by connecting with an empty URL.
- Run versioned schema migrations with `db.migrate`.
- Plan migrations without Postgres, using the catalog and planner only.

## Example
```rust
use quench_db::{Crud, Model};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Widget {
    id: String,
    name: String,
}

impl Model for Widget {
    fn table_name() -> String { "widgets".to_string() }
    fn columns() -> Vec<&'static str> { vec!["id", "name"] }
}

let db = quench_db::Db::connect(&url).await?;   // "" -> in-memory
let widgets = db.repository::<Widget>();
let w = Widget { id: "1".into(), name: "sprocket".into() };
widgets.create(&w).await?;
let read = widgets.read("1").await?;            // Option<Widget>
let some = widgets.find_by("name", "sprocket").await?;
widgets.delete("1").await?;
```

## Features
Default: `postgres`, `tls`.
- `postgres`: the Postgres backend, `Db::Postgres` and the migration runner. Without it only the in-memory backend, catalog and planner are built.
- `tls`: `sslmode=require` and `verify-full` for Postgres.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-db/
