---
title: Errors
parent: quench-sqlite
grand_parent: Crates
nav_order: 4
---

```rust
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    NotADatabase(String),
    Corrupt { page: u32, reason: String },
    Syntax { pos: usize, message: String },
    Constraint(String),
    KeyExists,
    Busy,
    ReadOnly(&'static str),
    Unsupported(String),
}
```

`Error` implements `Display` and `std::error::Error` (`source` is the `io::Error` for `Io`) and `From<std::io::Error>`.

| Variant | When | Text |
|---|---|---|
| `Io` | the operating system refused a read, write, sync, delete or lock | `i/o error: ...` |
| `NotADatabase` | the file is shorter than a header, its first 16 bytes are not `SQLite format 3\0`, the page size is not a power of two from 512 to 65536, the payload fractions are not 64/32/32, or too many bytes are reserved | `not a SQLite database: ...` |
| `Corrupt` | a page or record is malformed: a page type that is no b-tree page, a cell outside its page, an overflow chain that ends early or loops, a tree that is too deep (a loop), a page number out of range, a journal header that makes no sense. `page` is 0 when no page applies. Reading never panics on bad bytes. | `corrupt database, page N: ...` |
| `Syntax` | the SQL text is wrong; `pos` is the byte offset; also "no such table", "no such column", "ambiguous column name", a wrong number of values | `SQL syntax error at byte N: ...` |
| `Constraint` | `UNIQUE`, `NOT NULL`, `CHECK` broke; also "table x already exists", "cannot start a transaction within a transaction", "no such savepoint", "datatype mismatch" for a non-integer rowid | the SQLite wording |
| `KeyExists` | the storage layer: a rowid or index entry (or a schema name) is already there | `the key already exists` |
| `Busy` | another handle holds the file for writing (`open_rw`, `create`, `Connection::open`) | `the database is locked by another writer` |
| `ReadOnly` | a write on a `Database` opened with `open`, or outside a transaction | `read-only: ...` |
| `Unsupported` | valid SQLite that this crate does not do: UTF-16 text, a WAL-mode file, a file format number it does not know, a hot journal found by a reader, SQL outside the [supported subset](supported.md), a function it does not have, a database of 1 GiB or more that would be written | `unsupported: ...` |

A statement that returns an error has changed nothing: an autocommitted one rolled back, one inside `BEGIN` was undone alone.

After an `Io` error from `commit` (a full disk, a failed sync), the connection's in-memory state is discarded; the file holds either the old state or the new one, and the next `open` recovers whichever is needed.
