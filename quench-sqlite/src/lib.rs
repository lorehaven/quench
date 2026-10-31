//! SQLite database files, read and written by this crate's own code, with SQL on top: the file format's pages,
//! b-trees and records, a rollback journal that survives a crash, and an SQL engine for the statements an
//! application issues. No dependencies. Files made by the stock `sqlite3` open here, and files made here open
//! there.
//!
//! ```no_run
//! use quench_sqlite::{Connection, Value};
//!
//! let mut db = Connection::open_or_create("backup.db")?;
//! db.execute("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, size INTEGER)", &[])?;
//! db.execute("INSERT INTO items (name, size) VALUES (?, ?)", &[Value::Text("a.jpg".into()), Value::Integer(1024)])?;
//! let rows = db.query("SELECT name, size FROM items WHERE size > ? ORDER BY id", &[Value::Integer(100)])?;
//! for row in &rows.rows {
//!     println!("{:?}", row);
//! }
//! # Ok::<(), quench_sqlite::Error>(())
//! ```
//!
//! [`Database`] is the storage layer underneath (pages, trees, transactions); [`Connection`] is the SQL layer.
//! The plan and the list of what is not done are in `plans/SQLITE.md`.

// Binary formats pack small fields out of wider integers; each narrowing here is of a value the format (or a
// length check just above it) bounds.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    // Integers and reals compare as reals, as far as an f64 holds the integer.
    clippy::cast_precision_loss,
    // The executor follows SQLite's evaluation rules case by case: long matches and short names (`a`, `b`, `x`,
    // `y`) for the two sides of an operator read better than splitting them up.
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::option_if_let_else,
    // Float comparisons here are SQLite's: exact equality and exact constants (the 64-bit range limits).
    clippy::float_cmp,
    clippy::excessive_precision
)]

mod btree;
mod conn;
mod engine;
mod error;
mod header;
mod pager;
pub mod record;
mod schema;
pub mod sql;
mod tree;
pub mod varint;

use std::path::Path;

pub use conn::Connection;
pub use engine::exec::Rows;
pub use error::{Error, Result};
pub use header::Header;
pub use record::Value;
pub use schema::{Object, ObjectKind};
pub use tree::{compare_entries, compare_values};

use pager::Pager;

/// An open database file.
#[derive(Debug)]
pub struct Database {
    pager: Pager,
}

impl Database {
    /// Opens a database file for reading.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            pager: Pager::open(path)?,
        })
    }

    /// Opens a database file for reading and writing. The file is locked against other writers, and an
    /// interrupted write (a hot journal) is rolled back first.
    pub fn open_rw(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            pager: Pager::open_rw(path)?,
        })
    }

    /// Makes a new database file (it must not exist) and opens it for reading and writing. `page_size` is a power
    /// of two from 512 to 65536; 4096 is the usual.
    pub fn create(path: impl AsRef<Path>, page_size: u32) -> Result<Self> {
        Ok(Self {
            pager: Pager::create(path, page_size)?,
        })
    }

    /// Starts a transaction; every change below needs one.
    pub fn begin(&mut self) -> Result<()> {
        self.pager.begin()
    }

    /// Makes the changes of the transaction permanent.
    pub fn commit(&mut self) -> Result<()> {
        self.pager.commit()
    }

    /// Throws the changes of the transaction away.
    pub fn rollback(&mut self) {
        self.pager.rollback();
    }

    /// Runs `f` in a transaction: committed if it returns `Ok`, rolled back if it returns an error.
    pub fn write<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.begin()?;
        match f(self) {
            Ok(value) => {
                self.commit()?;
                Ok(value)
            }
            Err(e) => {
                self.rollback();
                Err(e)
            }
        }
    }

    /// Adds a table: an empty tree and its row in `sqlite_schema`. Returns the root page. `sql` is stored as
    /// given; this layer does not parse it.
    pub fn create_table(&mut self, name: &str, sql: &str) -> Result<u32> {
        schema::create(&mut self.pager, ObjectKind::Table, name, name, Some(sql))
    }

    /// Adds an index on `table`. Returns the root page. Existing rows are not indexed here.
    pub fn create_index(&mut self, name: &str, table: &str, sql: &str) -> Result<u32> {
        schema::create(&mut self.pager, ObjectKind::Index, name, table, Some(sql))
    }

    /// Removes a table or an index and frees its pages; false when there is none of that name.
    pub fn drop_object(&mut self, name: &str) -> Result<bool> {
        schema::drop(&mut self.pager, name)
    }

    /// Adds a row; the rowid must not be in use.
    pub fn insert_row(&mut self, root_page: u32, rowid: i64, values: &[Value]) -> Result<()> {
        tree::table_insert(
            &mut self.pager,
            root_page,
            rowid,
            &record::encode(values),
            false,
        )
    }

    /// Adds a row, replacing the one with that rowid if there is one.
    pub fn replace_row(&mut self, root_page: u32, rowid: i64, values: &[Value]) -> Result<()> {
        tree::table_insert(
            &mut self.pager,
            root_page,
            rowid,
            &record::encode(values),
            true,
        )
    }

    /// Removes a row; false when there was none.
    pub fn delete_row(&mut self, root_page: u32, rowid: i64) -> Result<bool> {
        tree::table_delete(&mut self.pager, root_page, rowid)
    }

    /// The rowid a new row takes: one more than the largest, 1 for an empty table.
    pub fn next_rowid(&mut self, root_page: u32) -> Result<i64> {
        tree::next_rowid(&mut self.pager, root_page)
    }

    /// Adds an entry (the indexed values and then the rowid) to an index.
    pub fn index_insert(&mut self, root_page: u32, entry: &[Value]) -> Result<()> {
        tree::index_insert(&mut self.pager, root_page, entry)
    }

    /// Removes an entry from an index; false when it was not there.
    pub fn index_delete(&mut self, root_page: u32, entry: &[Value]) -> Result<bool> {
        tree::index_delete(&mut self.pager, root_page, entry)
    }

    /// Sets `PRAGMA user_version`.
    pub fn set_user_version(&mut self, version: u32) -> Result<()> {
        self.pager.header_mut()?.user_version = version;
        Ok(())
    }

    /// The file header.
    #[must_use]
    pub const fn header(&self) -> &Header {
        self.pager.header()
    }

    /// Pages read from the file so far.
    #[cfg(test)]
    pub(crate) const fn file_reads(&self) -> u64 {
        self.pager.file_reads
    }

    /// Empties the page cache (tests).
    #[cfg(test)]
    pub(crate) fn drop_cache(&mut self) {
        self.pager.drop_cache();
    }

    /// The number of pages.
    pub(crate) const fn page_count(&self) -> u32 {
        self.pager.page_count()
    }

    /// Starts a statement inside a transaction; see [`abort_statement`](Self::abort_statement).
    pub(crate) fn begin_statement(&mut self) -> Result<()> {
        self.pager.begin_statement()
    }

    /// The statement succeeded.
    pub(crate) fn end_statement(&mut self) {
        self.pager.end_statement();
    }

    /// The statement failed: undo it alone.
    pub(crate) fn abort_statement(&mut self) {
        self.pager.abort_statement();
    }

    /// A point to come back to inside the open transaction.
    pub(crate) fn snapshot(&self) -> Result<pager::Snapshot> {
        self.pager.snapshot()
    }

    /// Goes back to a snapshot.
    pub(crate) fn restore(&mut self, snapshot: pager::Snapshot) -> Result<()> {
        self.pager.restore(snapshot)
    }

    /// Checks every tree and page; the list of problems, empty when the database is sound.
    pub fn integrity_check(&mut self) -> Result<Vec<String>> {
        engine::check::integrity(&mut self.pager)
    }

    /// The pager, for the executor.
    pub(crate) const fn pager_mut(&mut self) -> &mut Pager {
        &mut self.pager
    }

    /// Every table, index, view and trigger.
    pub fn objects(&mut self) -> Result<Vec<Object>> {
        schema::read(&mut self.pager)
    }

    /// The tables (not the internal `sqlite_` ones).
    pub fn tables(&mut self) -> Result<Vec<Object>> {
        Ok(self
            .objects()?
            .into_iter()
            .filter(|o| o.kind == ObjectKind::Table && !o.name.starts_with("sqlite_"))
            .collect())
    }

    /// Calls `f(rowid, values)` for every row of the table tree rooted at `root_page`, in rowid order.
    pub fn scan_table(
        &mut self,
        root_page: u32,
        f: &mut dyn FnMut(i64, Vec<Value>) -> Result<()>,
    ) -> Result<()> {
        btree::scan_table(&mut self.pager, root_page, &mut |rowid, payload| {
            f(rowid, record::decode(payload)?)
        })
    }

    /// The row with `rowid` in the table tree rooted at `root_page`.
    pub fn get_row(&mut self, root_page: u32, rowid: i64) -> Result<Option<Vec<Value>>> {
        btree::table_get(&mut self.pager, root_page, rowid)?
            .map(|payload| record::decode(&payload))
            .transpose()
    }

    /// Calls `f(entry)` for every entry of the index tree rooted at `root_page`, in key order. An entry is the
    /// indexed values followed by the rowid.
    pub fn scan_index(
        &mut self,
        root_page: u32,
        f: &mut dyn FnMut(Vec<Value>) -> Result<()>,
    ) -> Result<()> {
        btree::scan_index(&mut self.pager, root_page, &mut |payload| {
            f(record::decode(payload)?)
        })
    }
}

#[cfg(test)]
mod check_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod sql_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod write_tests;

#[cfg(test)]
quench_test::main!();
