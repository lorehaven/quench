---
title: quench-sqlite
parent: Crates
nav_order: 62
has_children: true
permalink: /crates/quench-sqlite/
---

`quench-sqlite` (crate `quench_sqlite`) reads and writes **SQLite 3 database files with its own code**: the pager and the rollback journal, the b-trees, the record format, and an SQL engine for the statements an application issues. A file made by the stock `sqlite3` opens here, and a file made here opens in `sqlite3` (`PRAGMA integrity_check` is `ok`). It has **no dependencies** (the dev-dependencies are `quench-core` and `quench-test`) and replaces `rusqlite` and `libsqlite3-sys` for the supported subset.

* Version: `0.1.0`. Edition 2024. No features. `[lints] workspace = true` (`unsafe_code = "deny"`; none present).
* Two layers: [`Connection`](api.md#connection) runs SQL; [`Database`](api.md#database) is the storage layer under it (transactions, rows, index entries, schema objects by hand).
* Not a drop-in for all of SQLite: the supported SQL is listed in [supported SQL](supported.md), and the file features that are not done (WAL mode, `AUTO_VACUUM`, UTF-16 text, databases of 1 GiB and more for writing) are refused with an error that names them.
* One writer per file (an exclusive file lock), readers take no lock. No threads of its own; a `Connection` is `Send` but not shared.

## Install

```toml
[dependencies]
quench-sqlite = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Where to go next

* [Usage](usage.md): connect, create tables, parameters, transactions, upserts, `RETURNING`, migrations by hand.
* [API reference](api.md)
* [Supported SQL](supported.md): what runs, what is refused, and the differences from SQLite.
* [Errors](errors.md)
* [Internals](internals.md): the file format as implemented, the journal and crash recovery, the planner, limits.
* [Testing](testing.md): the differential tests against the stock `sqlite3` and the crash tests.
* [quench-db](../quench-db/index.md) has a `sqlite` feature that puts this crate behind its `Database` and `Crud` traits.

## Minimal example

```rust
use quench_sqlite::{Connection, Value};

let mut db = Connection::open_or_create("backup.db")?;
db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, size INTEGER)", &[])?;
db.execute("INSERT INTO items (name, size) VALUES (?, ?)", &[Value::Text("a.jpg".into()), Value::Integer(1024)])?;
let rows = db.query("SELECT name, size FROM items WHERE size > ? ORDER BY id", &[Value::Integer(100)])?;
assert_eq!(rows.columns, ["name", "size"]);
# Ok::<(), quench_sqlite::Error>(())
```
