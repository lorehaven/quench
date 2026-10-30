---
title: Data access
parent: Guides
nav_order: 6
---

How the data crates fit together: [quench-db](../crates/quench-db/index.md) (traits, models, migrations), [quench-pg](../crates/quench-pg/index.md) (PostgreSQL driver), [quench-redis](../crates/quench-redis/index.md) (Redis client) and [quench-cache](../crates/quench-cache/index.md) (key/value cache over memory or Redis).

## Layers

* `quench-pg` speaks the PostgreSQL wire protocol (`Pool`, `Row`, transactions, SCRAM). Use it directly for hand-written SQL.
* `quench-db` adds `Model`, `Crud<T>`, `Db`/`Repository<T>`, an in-memory backend and a Postgres backend (on `quench-pg`), and the migration system.
* `quench-redis` is a RESP client (`Redis`, `Client`, cluster).
* `quench-cache` offers `CacheStore` (in-process or Redis, chosen from `CACHE_URL`/`REDIS_URL`), `DataCache`, `RequestCache`.

## Models and CRUD

```rust
use quench_db::{Crud, Db, Model};
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Widget { id: String, name: String }

impl Model for Widget {
    fn table_name() -> String { "widgets".to_string() }
    fn columns() -> Vec<&'static str> { vec!["id", "name"] }
}

let db = Db::connect(&url).await?;      // "" gives the in-memory backend
let widgets = db.repository::<Widget>();
widgets.create(&Widget { id: "1".into(), name: "sprocket".into() }).await?;
let found = widgets.read("1").await?;   // Option<Widget>
```

`Model` requires `Clone`; the primary key name defaults to `"id"`; `read` and `delete` take `&str`. The backends differ in edge cases (in-memory `update` of a missing row upserts, Postgres returns `NotFound`; `find_by` matches only string columns in memory). Table and key names are pasted into SQL unquoted. See [quench-db usage](../crates/quench-db/usage.md) and [backends](../crates/quench-db/api-backends.md). The shipped example is `examples/db_example`.

## Migrations

Migration files are TOML in a catalog directory of modules (`module.toml` plus numbered files with `[[migrations]]`, `[[migrations.changes]]`). `Catalog::load(dir)`, `MigrationPlan::resolve(...)`, and a `MigrationRunner` apply them; applied ids are recorded in the table named by `DB_MIGRATION_TABLE` (default `quench_migrations`), each migration in its own transaction. `Database::migrate(Vec<Migration>)` is the simple path (no `${var}` substitution, ignores `since` on Postgres). `MigrationRunner::reset` ignores `dry_run` (suspected bug). File formats: [migration files](../crates/quench-db/api-migrations.md), [catalog](../crates/quench-db/api-catalog.md), [plan](../crates/quench-db/api-plan.md), [runner](../crates/quench-db/api-runner.md).

## Raw PostgreSQL

```rust
let pool = quench_pg::Pool::connect("postgres://app:secret@db:5432/app?sslmode=require").await?;
let rows = pool.query("SELECT id, name FROM people WHERE age > $1", &[&30i32]).await?;
let id: i32 = rows[0].get("id")?;
let mut conn = pool.get().await?;
let mut tx = conn.begin().await?;
tx.execute("INSERT INTO t VALUES ($1)", &[&3i32]).await?;
tx.commit().await?;
```

Details and type mapping: [quench-pg usage](../crates/quench-pg/usage.md), [types](../crates/quench-pg/api-types.md). Server errors expose `is_unique_violation()` and `code()`.

## Redis

```rust
use quench_redis::{Redis, cmd};
let redis = Redis::connect("redis://localhost:6379").await?;
redis.exec::<()>(&cmd("SET").arg("k").arg("v").arg("EX").arg(&60)).await?;
let v: Option<String> = redis.exec(&cmd("GET").arg("k")).await?;
```

Pipelines and pooled connections: `Client`; cluster: `Redis::connect_cluster`. See [quench-redis usage](../crates/quench-redis/usage.md).

## Cache

```rust
let store = quench_cache::CacheStore::from_env("billing").await?;
store.set("user:1", quench_serde::json!({"name": "Ann"}), Some(300)).await?;
let user = store.get("user:1").await?;
```

With neither `CACHE_URL` nor `REDIS_URL` the store is in-process (per process, lost on restart; `is_shared()` tells). Always give each service its own prefix: `clear()` with an empty prefix deletes the whole Redis database. See [quench-cache usage](../crates/quench-cache/usage.md).

## Configuration summary

Environment variables per crate are listed in each crate's `configuration.md`: [db](../crates/quench-db/configuration.md), [pg](../crates/quench-pg/configuration.md), [redis](../crates/quench-redis/configuration.md), [cache](../crates/quench-cache/configuration.md).
