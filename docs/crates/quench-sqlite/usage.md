---
title: Usage
parent: quench-sqlite
grand_parent: Crates
nav_order: 1
---

## 1. Open or create a database

```rust
use quench_sqlite::Connection;

let mut db = Connection::open_or_create("app.db")?;   // makes the file (4096-byte pages) if it is not there
let mut existing = Connection::open("other.db")?;     // fails if it is not there
let mut fresh = Connection::create("new.db")?;        // fails if it is there
# Ok::<(), quench_sqlite::Error>(())
```

Opening takes an exclusive lock on the file: a second `open` of the same file from another handle fails with `Error::Busy` until the first is dropped. If the last writer crashed, the first open **recovers** the file (see [internals](internals.md#crash-recovery)) before it does anything else.

## 2. Create tables and run statements

```rust
use quench_sqlite::{Connection, Value};
let mut db = Connection::open_or_create("app.db")?;
db.execute(
    "CREATE TABLE IF NOT EXISTS items (
        category TEXT NOT NULL, local_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
        attempts INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (category, local_id))",
    &[],
)?;
db.execute("CREATE INDEX IF NOT EXISTS items_status ON items (status, attempts)", &[])?;
let changed = db.execute(
    "INSERT INTO items (category, local_id) VALUES (?, ?)",
    &[Value::Text("photo".into()), Value::Text("id1".into())],
)?;
assert_eq!(changed, 1);
assert_eq!(db.last_insert_rowid(), 1);
# Ok::<(), quench_sqlite::Error>(())
```

`execute` and `query` take one or several statements separated by `;`. `execute` returns the rows changed by the last statement; `query` returns the rows of the last one. Each statement outside a transaction is its own transaction: it is all there or not at all.

## 3. Parameters

Positional parameters are `?` (the next number) and `?N`; named ones are `:name`, `@name` and `$name`.

```rust
use quench_sqlite::{Connection, Value};
# let mut db = Connection::open_or_create("app.db")?;
let rows = db.query(
    "SELECT local_id FROM items WHERE category = ?1 AND attempts <= ?2 ORDER BY local_id LIMIT 50",
    &[Value::Text("photo".into()), Value::Integer(3)],
)?;
let rows = db.query_named(
    "SELECT * FROM items WHERE status = :status",
    &[],
    &[(":status", Value::Text("done".into()))],
)?;
for row in &rows.rows {
    let id: Option<&str> = row[1].as_text();
    println!("{id:?}");
}
# Ok::<(), quench_sqlite::Error>(())
```

A value is never spliced into the SQL, so there is nothing to escape. A parameter with no value is an error (`Unsupported`, naming it).

## 4. Transactions

```rust
use quench_sqlite::{Connection, Value};
# let mut db = Connection::open_or_create("app.db")?;
db.transaction(|db| {
    for i in 0..1000 {
        db.execute("INSERT INTO items (category, local_id) VALUES ('doc', ?)", &[Value::Text(format!("id{i}"))])?;
    }
    Ok(())
})?;   // committed; an Err from the closure rolls everything back
# Ok::<(), quench_sqlite::Error>(())
```

Many inserts in one transaction are far faster than many autocommitted statements: a commit writes a journal and syncs twice. `BEGIN`, `COMMIT`, `ROLLBACK`, `SAVEPOINT`, `RELEASE` and `ROLLBACK TO` also work as statements. Inside a transaction a statement that fails (a constraint, for instance) undoes only itself; the transaction goes on.

## 5. Upserts, `OR REPLACE` and `RETURNING`

```rust
use quench_sqlite::{Connection, Value};
# let mut db = Connection::open_or_create("app.db")?;
db.execute(
    "INSERT INTO items (category, local_id, attempts) VALUES ('photo', 'id1', 1)
     ON CONFLICT (category, local_id) DO UPDATE SET attempts = attempts + excluded.attempts",
    &[],
)?;
let r = db.query("UPDATE items SET status = 'done' WHERE local_id = 'id1' RETURNING category, status", &[])?;
assert_eq!(r.columns, ["category", "status"]);
# Ok::<(), quench_sqlite::Error>(())
```

## 6. A schema that evolves

`PRAGMA user_version` is a number the application owns; use it as the schema version.

```rust
use quench_sqlite::{Connection, Value};
# let mut db = Connection::open_or_create("app.db")?;
let version = db.query("PRAGMA user_version", &[])?.rows[0][0].as_integer().unwrap_or(0);
if version < 1 {
    db.transaction(|db| {
        db.execute("ALTER TABLE items ADD COLUMN error TEXT", &[])?;
        db.execute("PRAGMA user_version = 1", &[])?;
        Ok(())
    })?;
}
# Ok::<(), quench_sqlite::Error>(())
```

`ALTER TABLE ... ADD COLUMN` is the only `ALTER` there is (see [supported SQL](supported.md)); existing rows read the column's default without being rewritten.

## 7. Check a file

```rust
use quench_sqlite::Database;
let mut db = Database::open("app.db")?;
let problems = db.integrity_check()?;
assert!(problems.is_empty(), "{problems:?}");
# Ok::<(), quench_sqlite::Error>(())
```

`PRAGMA integrity_check` through a `Connection` gives the same as rows (`ok`, or one row per problem). A file with a hot journal is not opened for reading: open it with `Connection::open` or `Database::open_rw` to recover it.

## 8. The storage layer by hand

```rust
use quench_sqlite::{Database, Value};
let mut db = Database::create("raw.db", 4096)?;
let root = db.write(|db| db.create_table("kv", "CREATE TABLE kv (k INTEGER PRIMARY KEY, v)"))?;
db.write(|db| db.insert_row(root, 1, &[Value::Null, Value::Text("one".into())]))?;
let row = db.get_row(root, 1)?;
assert_eq!(row, Some(vec![Value::Null, Value::Text("one".into())]));   // the rowid alias is stored as NULL
# Ok::<(), quench_sqlite::Error>(())
```

This is what `Connection` builds on. It does not parse the `CREATE TABLE` text you give it, apply affinity or enforce constraints; use it for tools that read and repair files.

## Pitfalls

* A `REAL` is stored as a real, even when it is whole; a file made by `sqlite3` may hold the same value as an integer, and the column's affinity turns it back into a real on reading.
* `'abc' = 'ABC'` is false (BINARY collation); declare `COLLATE NOCASE` on the column for the other. An index on a column with a collation other than BINARY is refused.
* A query reads its rows in batches, then runs the rest of the pipeline on them; a `SELECT` over a table of millions of rows with no `LIMIT` builds the whole result in memory. Use `LIMIT` and key ranges (`WHERE id > ? ORDER BY id LIMIT 500`).
