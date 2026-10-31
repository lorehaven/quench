# quench-pg
A PostgreSQL client: the v3 wire protocol with SCRAM-SHA-256 and TLS, typed rows through `quench-serde`, transactions and a connection pool.

## What it is
`quench-pg` is a PostgreSQL client written against `tokio`: the v3 wire protocol over TCP, Unix sockets or TLS, SCRAM-SHA-256 authentication, parameterised queries with typed rows, transactions and a connection pool. It replaces `sqlx` and `tokio-postgres` in the workspace.

Rows are read through `quench-serde`, which is how `quench-db` maps models without a derive. Everything travels in PostgreSQL's text format; there is no prepared-statement cache, `COPY` or `LISTEN` support.

## Use cases
- Run parameterised queries and read columns by name or position.
- Deserialize rows straight into structs with `quench-serde`.
- Run migrations and DDL scripts with `batch_execute`.
- Share a bounded pool of connections across async tasks.

## Example
```rust
use quench_pg::Pool;
use quench_serde::Deserialize;

#[derive(Deserialize)]
struct Person { id: i32, name: String }

let pool = Pool::connect("postgres://app:secret@db.internal:5432/app?sslmode=require").await?;

let rows = pool.query("SELECT id, name FROM people WHERE age > $1", &[&30i32]).await?;
let people: Vec<Person> = rows.iter().map(|r| r.deserialize::<Person>()).collect::<Result<_, _>>()?;

let removed: u64 = pool.execute("DELETE FROM people WHERE id = $1", &[&7i64]).await?;
```

## Features
- `tls` (default): `sslmode=require` and `verify-full` through `quench-tls`. Without it those modes fail to connect.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-pg/
