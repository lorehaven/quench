---
title: API reference
parent: quench-sqlite
grand_parent: Crates
nav_order: 2
---

Everything below is exported from the crate root unless a module is named. All fallible calls return `quench_sqlite::Result<T>` (`std::result::Result<T, Error>`, see [errors](errors.md)).

## Value

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value { Null, Integer(i64), Real(f64), Text(String), Blob(Vec<u8>) }

impl Value {
    pub const fn as_integer(&self) -> Option<i64>   // Some for Integer only
    pub fn as_text(&self) -> Option<&str>           // Some for Text only
}
```

SQLite's five storage classes. There is no `Bool`: a boolean is `Integer(0)` or `Integer(1)`.

```rust
pub fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering
pub fn compare_entries(a: &[Value], b: &[Value]) -> std::cmp::Ordering
```

SQLite's order with the BINARY collation: NULL, then numbers (integers and reals compared as numbers, through `f64` when the kinds differ), then text (bytes), then blobs. An entry that is a prefix of another is the smaller. This is the order of index trees.

## Connection

```rust
pub struct Connection { /* Debug */ }

impl Connection {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>             // existing file; locks it; recovers a hot journal
    pub fn create(path: impl AsRef<Path>) -> Result<Self>           // new file, 4096-byte pages; fails if it exists
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self>

    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64>
    pub fn execute_named(&mut self, sql: &str, positional: &[Value], named: &[(&str, Value)]) -> Result<u64>
    pub fn query(&mut self, sql: &str, params: &[Value]) -> Result<Rows>
    pub fn query_named(&mut self, sql: &str, positional: &[Value], named: &[(&str, Value)]) -> Result<Rows>
    pub fn transaction<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T>

    pub const fn last_insert_rowid(&self) -> i64
    pub const fn changes(&self) -> u64
    pub const fn in_transaction(&self) -> bool
}
```

* `execute` and `query` accept several statements separated by `;` and run them in order, stopping at the first error. `execute` returns the rows changed by the **last** statement; `query` returns the rows of the **last** statement (empty `Rows` for a statement that returns none).
* Outside a `BEGIN`, a statement that writes runs in its own transaction; one that fails leaves no trace. Inside a `BEGIN`, a failing statement is undone alone.
* Named parameters match by name without the `:`/`@`/`$` in the SQL; the names in `named` may be given with or without the prefix.
* `transaction` runs `BEGIN`, the closure, and `COMMIT` (or `ROLLBACK` if the closure returned an error, whose error is returned). It does not nest; a `BEGIN` inside it is an error.
* `changes` is the count for the last statement that ran; `last_insert_rowid` is 0 until a row has been inserted on this connection.
* Dropping a connection inside an open transaction discards the transaction.

### Rows

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rows { pub columns: Vec<String>, pub rows: Vec<Vec<Value>> }
```

`columns` are the result column names: the alias when there is one, a column's name for a plain column, otherwise a short rendering of the expression.

### PRAGMAs

| Pragma | Read | Write |
|---|---|---|
| `user_version` | the number in the header | `PRAGMA user_version = n` (a write; runs in the open transaction or its own) |
| `schema_version`, `page_count`, `page_size`, `freelist_count` | the header values | not allowed |
| `encoding` | `UTF-8` | not allowed |
| `journal_mode` | `delete` | `= delete` only |
| `foreign_keys` | `0` | `= 0` only |
| `synchronous`, `cache_size`, `busy_timeout`, `temp_store`, `locking_mode` | no row | accepted and ignored |
| `table_info(t)` | rows `cid, name, type, notnull, dflt_value, pk` | |
| `integrity_check`, `quick_check` | `ok`, or one row per problem (at most 100) | |

Any other pragma is `Error::Unsupported`.

## Database

The storage layer. A `Database` knows pages, trees, records and the schema table, and nothing of SQL.

```rust
pub struct Database { /* Debug */ }

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>      // read only; no lock; refuses a hot journal
    pub fn open_rw(path: impl AsRef<Path>) -> Result<Self>   // exclusive lock; recovers a hot journal
    pub fn create(path: impl AsRef<Path>, page_size: u32) -> Result<Self>  // power of two, 512..=65536

    pub fn begin(&mut self) -> Result<()>
    pub fn commit(&mut self) -> Result<()>
    pub fn rollback(&mut self)
    pub fn write<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T>

    pub fn create_table(&mut self, name: &str, sql: &str) -> Result<u32>          // root page
    pub fn create_index(&mut self, name: &str, table: &str, sql: &str) -> Result<u32>
    pub fn drop_object(&mut self, name: &str) -> Result<bool>
    pub fn insert_row(&mut self, root_page: u32, rowid: i64, values: &[Value]) -> Result<()>
    pub fn replace_row(&mut self, root_page: u32, rowid: i64, values: &[Value]) -> Result<()>
    pub fn delete_row(&mut self, root_page: u32, rowid: i64) -> Result<bool>
    pub fn next_rowid(&mut self, root_page: u32) -> Result<i64>
    pub fn index_insert(&mut self, root_page: u32, entry: &[Value]) -> Result<()>
    pub fn index_delete(&mut self, root_page: u32, entry: &[Value]) -> Result<bool>
    pub fn set_user_version(&mut self, version: u32) -> Result<()>

    pub const fn header(&self) -> &Header
    pub fn objects(&mut self) -> Result<Vec<Object>>
    pub fn tables(&mut self) -> Result<Vec<Object>>           // without the sqlite_ internal ones
    pub fn scan_table(&mut self, root_page: u32, f: &mut dyn FnMut(i64, Vec<Value>) -> Result<()>) -> Result<()>
    pub fn get_row(&mut self, root_page: u32, rowid: i64) -> Result<Option<Vec<Value>>>
    pub fn scan_index(&mut self, root_page: u32, f: &mut dyn FnMut(Vec<Value>) -> Result<()>) -> Result<()>
    pub fn integrity_check(&mut self) -> Result<Vec<String>>
}
```

* Every method that changes something needs an open transaction (`begin`, or inside `write`); otherwise `Error::ReadOnly`. All changes of a transaction stay in memory until `commit`.
* `create_table` and `create_index` store `sql` as given and make an empty tree; they do not parse it, make the automatic indexes of its constraints, or fill an index. The names must be new (`Error::KeyExists`).
* `insert_row` fails with `Error::KeyExists` when the rowid is taken; `replace_row` replaces. `values` is the record as stored: **an `INTEGER PRIMARY KEY` column holds `Value::Null`**, its value is the rowid.
* An index entry is the indexed values followed by the rowid, in the key order of `compare_entries`. `index_insert` of an entry already there is `Error::KeyExists`.
* `scan_table` and `scan_index` visit in key order; an error from `f` stops the scan and is returned.
* `drop_object` frees the object's pages (into the freelist) and returns false for an unknown name; names are case blind.

### Header, Object, ObjectKind

```rust
pub struct Header {                // every field is pub; see the file format for the meaning
    pub page_size: u32, pub write_version: u8, pub read_version: u8, pub reserved: u8,
    pub change_counter: u32, pub page_count: u32, pub freelist_trunk: u32, pub freelist_count: u32,
    pub schema_cookie: u32, pub schema_format: u32, pub text_encoding: u32, pub user_version: u32,
    pub version_valid_for: u32,
}
impl Header { pub fn parse(bytes: &[u8]) -> Result<Self> }      // the first 100 bytes of a file

pub enum ObjectKind { Table, Index, View, Trigger }
pub struct Object { pub kind: ObjectKind, pub name: String, pub table: String, pub root_page: u32, pub sql: Option<String> }
```

`Object` is a row of `sqlite_schema`; `sql` is `None` for the automatic indexes of `UNIQUE` and `PRIMARY KEY` constraints.

## record and varint

```rust
pub mod record {
    pub fn decode(record: &[u8]) -> Result<Vec<Value>>
    pub fn encode(values: &[Value]) -> Vec<u8>
}
pub mod varint {
    pub fn get(buf: &[u8]) -> Option<(u64, usize)>    // the value and the bytes it took; None if buf ends first
    pub const fn len(value: u64) -> usize             // 1 to 9
    pub fn put(out: &mut Vec<u8>, value: u64)
}
```

The record format of SQLite (a header of serial types, then the values) and its 1-to-9-byte big-endian varints. `decode` fails with `Error::Corrupt` on a malformed record (never panics); `encode` picks the smallest integer width.

## sql

```rust
pub mod sql {
    pub mod ast { /* the syntax tree: Stmt, Select, Expr, ... */ }
    pub fn parse(sql: &str) -> Result<Vec<ast::Stmt>>
    pub fn parse_one(sql: &str) -> Result<ast::Stmt>
    pub fn parse_with_text(sql: &str) -> Result<Vec<(ast::Stmt, String)>>   // each statement with its own text
}
```

The parser behind `Connection`. A syntax error is `Error::Syntax { pos, message }` with the byte offset; syntax outside the supported subset is `Error::Unsupported` naming it. See [supported SQL](supported.md). The `ast` types are public but not covered by a stability promise before 1.0.
