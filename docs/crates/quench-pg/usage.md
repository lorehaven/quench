---
title: Usage
parent: quench-pg
grand_parent: Crates
nav_order: 1
---

Examples are checked against `tests/live.rs` and `tests/fake.rs` where a test exists.

## 1. Connect with a pool

```rust
use quench_pg::{Pool, PoolOptions, Config};

let pool = Pool::connect("postgres://app:secret@db.internal:5432/app?sslmode=require").await?;

// explicit options
let pool = Pool::connect_with(
    Config::from_url("postgres://app@db/app")?,
    PoolOptions { max_size: 10, ..PoolOptions::default() },
).await?;

// no connection until first use
let pool = Pool::lazy(Config::from_url("postgres://app@db/app")?, PoolOptions::default());
```

`connect` and `connect_with` open one connection immediately, so a bad URL or password fails there. Pitfalls: the password goes in the URL, percent-encoded (`@` as `%40`); `sslmode` defaults to `prefer`, which does not verify the certificate.

## 2. Query with parameters and read columns

```rust
let rows = pool.query("SELECT id, name, born FROM people WHERE name = $1 AND age > $2", &[&"ann", &30i32]).await?;
for row in &rows {
    let id: i32 = row.get("id")?;
    let name: String = row.get(&1usize)?;          // by position
    let born: Option<quench_core::time::Time> = row.get("born")?;   // NULL-able
}
let one  = pool.query_one("SELECT count(*) FROM people", &[]).await?;   // exactly one row
let maybe = pool.query_opt("SELECT * FROM people WHERE id = $1", &[&7i64]).await?;
let n: u64 = pool.execute("DELETE FROM people WHERE id = $1", &[&7i64]).await?;   // rows affected
```

Pitfalls: the index of `get` is `&str`, `&String`, or `&usize` (a bare integer literal does not infer; write `&0usize`). Reading NULL as a non-`Option` type is `Error::Decode`. `count(*)` is `int8`: read it as `i64` (an `i32` read of an `int8` works only while the text parses).

## 3. Rows into structs

```rust
use quench_serde::Deserialize;

#[derive(Deserialize)]
struct Person { id: i32, name: String }

let people: Vec<Person> = rows.iter().map(|r| r.deserialize::<Person>()).collect::<Result<_, _>>()?;
let pair: (String, i64) = row.deserialize_tuple()?;     // positional
```

`deserialize` builds an object keyed by column name and deserialises it; `Time` fields read `timestamptz` columns, `Value` fields read `jsonb`, `Vec<_>` fields read arrays. See [types](api-types.md).

## 4. Scripts and DDL

```rust
pool.batch_execute("CREATE TABLE t (id int PRIMARY KEY); INSERT INTO t VALUES (1); INSERT INTO t VALUES (2)").await?;
let results = pool.simple_query("SELECT 1; SELECT 'a'").await?;   // Vec<QueryResult>, one per statement
```

These use the simple protocol: no parameters, any number of `;`-separated statements. The extended protocol (`query`, `execute`) refuses multi-statement text.

## 5. Transactions

```rust
let mut conn = pool.get().await?;
let mut tx = conn.begin().await?;
tx.execute("INSERT INTO t VALUES ($1)", &[&3i32]).await?;
tx.commit().await?;       // or tx.rollback().await?
```

Dropping `tx` unfinished does not roll back at once: it flags the connection, which sends `ROLLBACK` before its next statement; a pooled connection returned with the transaction still open is discarded instead of reused.

## 6. Arrays, JSON, bytes, NULL

```rust
use quench_pg::PgArray;
use quench_serde::Value;

pool.execute("INSERT INTO t (tags, doc, blob, note) VALUES ($1, $2, $3, $4)",
    &[&PgArray(vec!["a", "b"]), &quench_serde::json::parse(r#"{"k":1}"#)?, &vec![0xde_u8, 0xad], &None::<String>]).await?;
let ids = pool.query("SELECT * FROM t WHERE id = ANY($1)", &[&PgArray(vec![1i32, 2, 3])]).await?;
```

Parameters are sent untyped; the server infers each from context (`$1` is `text[]` here because of the column). Pitfall: where the context does not decide a type (`SELECT $1`), the server defaults it to `text`.

## 7. Server errors

```rust
match pool.execute("INSERT INTO users (id) VALUES ($1)", &[&1i32]).await {
    Err(e) if e.is_unique_violation() => { /* SQLSTATE 23505 */ }
    Err(e) => println!("{:?} {:?}", e.code(), e.server().map(|s| &s.constraint)),
    Ok(_) => {}
}
```

The connection stays usable after a server error.

## 8. Timeouts and Unix sockets

```rust
let pool = Pool::connect("postgres://app@%2Fvar%2Frun%2Fpostgresql/app?connect_timeout=3&statement_timeout=2500").await?;
```

`statement_timeout` is enforced by the client: on expiry the call returns `Error::Timeout("running the statement")` and the connection is closed (the pool discards it). A host starting with `/` is a socket directory (file `.s.PGSQL.{port}`).

## 9. Health checks and server info

```rust
let mut conn = pool.get().await?;
conn.ping().await?;
let version = conn.parameter("server_version");   // Option<&str>
let pid = conn.process_id();
```
