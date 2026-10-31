# quench-sqlite

SQLite database files read and written by our own code: pager, b-trees, a crash-safe rollback journal and an SQL engine, with no dependencies.

## What it is

`quench-sqlite` implements the SQLite 3 file format and, on top of it, the SQL an application issues: tables, indexes, inserts with upserts and `RETURNING`, updates, deletes, selects with joins, grouping, subqueries and compound queries, transactions and savepoints. A file made by the stock `sqlite3` opens here, and a file made here passes `sqlite3`'s `PRAGMA integrity_check`. A commit survives a crash: the journal is in SQLite's own format, and the next open rolls an interrupted write back.

It is not all of SQLite: no WAL mode, no `AUTOINCREMENT`, views, triggers, foreign keys, date functions or CTEs yet (the full list is in the documentation). What is refused is refused with an error that names it.

## Use cases

- A small embedded database for an app or a tool (a backup manifest, a queue, a cache of lookups) without a C library.
- Reading and repairing SQLite files from a program: pages, trees, records and the schema are public.
- The SQLite backend of `quench-db` (feature `sqlite`), so a service can run on a file as easily as on Postgres.
- Checking a database file with a real `integrity_check` (every tree, every page, every index against its table).

## Example

```rust
use quench_sqlite::{Connection, Value};

let mut db = Connection::open_or_create("backup.db")?;
db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, size INTEGER)", &[])?;
db.transaction(|db| {
    db.execute("INSERT INTO items (name, size) VALUES (?, ?)", &[Value::Text("a.jpg".into()), Value::Integer(1024)])?;
    db.execute("INSERT INTO items (name, size) VALUES (?, ?)", &[Value::Text("b.jpg".into()), Value::Integer(2048)])?;
    Ok(())
})?;
let rows = db.query("SELECT name, size FROM items WHERE size > ? ORDER BY id", &[Value::Integer(1500)])?;
assert_eq!(rows.rows[0][0], Value::Text("b.jpg".into()));
# Ok::<(), quench_sqlite::Error>(())
```

## Documentation

Full documentation: https://lorehaven.github.io/quench/crates/quench-sqlite/
