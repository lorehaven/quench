---
title: Internals
parent: quench-sqlite
grand_parent: Crates
nav_order: 5
---

## Layers

| Module | Does |
|---|---|
| `pager` | the file as numbered pages (page 1 first); a cache; transactions; the rollback journal; the freelist; recovery |
| `header` | the 100-byte header |
| `btree`, `tree` | reading and changing b-trees: cells, overflow pages, splitting, merging, ranged scans, the integrity walk |
| `record`, `varint` | the record format |
| `schema` | `sqlite_schema` (the table on page 1) |
| `sql` | lexer, syntax tree, parser |
| `engine` | affinity and comparisons (`value`), operators and functions (`ops`), the catalog built from the stored `CREATE` text (`catalog`), expression evaluation (`eval`), `SELECT` and the planner (`exec`), `INSERT`/`UPDATE`/`DELETE` (`dml`), `CREATE`/`DROP`/`ALTER` (`ddl`), `integrity_check` (`check`) |
| `conn` | `Connection`: statements, transactions, savepoints, pragmas |

## The file

The format is SQLite 3's, for databases with these properties: rollback-journal mode (`read_version`/`write_version` 1), UTF-8 text, no auto-vacuum, schema formats 1 to 4.

* **Header** (bytes 0-99, big-endian): text `SQLite format 3\0`; page size at 16 (2 bytes, 1 meaning 65536); the versions at 18/19; reserved bytes per page at 20; the payload fractions 64/32/32 at 21-23; the change counter at 24; the size in pages at 28 (trusted only when the "version-valid-for" number at 92 equals the change counter, else the file length decides); the freelist trunk at 32 and its count at 36; the schema cookie at 40; the schema format at 44; the text encoding at 56; `user_version` at 60; `version-valid-for` at 92; the SQLite version number at 96 (3045000 in files made here).
* **Pages**: b-tree pages (types 13 table leaf, 5 table interior, 10 index leaf, 2 index interior), overflow pages (next page number, then data), freelist trunk pages (next trunk, count, leaf page numbers) and free leaf pages. The b-tree header of page 1 follows the file header.
* **Cells**: table leaf = varint payload size, varint rowid, payload; table interior = 4-byte left child, varint key; index leaf = varint payload size, payload; index interior = left child, then an index leaf cell. A payload larger than the page allows keeps `M + (P - M) mod (U - 4)` bytes (or `M` when that exceeds the maximum `X`) on the page and the rest on a chain of overflow pages; `U` is the usable size, `X = U - 35` for tables and `(U - 12) * 64 / 255 - 23` for indexes, `M = (U - 12) * 32 / 255 - 23`.
* **Table keys**: an interior cell's key is the largest rowid in its left subtree. An index entry is the indexed values followed by the rowid, so every entry is unique.
* **Schema rows** are `(type, name, tbl_name, rootpage, sql)`. The automatic indexes of `UNIQUE` and `PRIMARY KEY` constraints are rows with a NULL `sql`, named `sqlite_autoindex_<table>_<n>`, numbered in the order the constraints are declared (column constraints first, then table constraints); a constraint that repeats the columns of an earlier one makes no second index. The catalog pairs them with the constraints by that order.

## Writing

A page that changes is **rebuilt whole** from its cells: the cells are packed at the end of the page in order, with no free blocks and no fragmented bytes. Files made by SQLite have free blocks; they are read as they are and rewritten compact when their page changes.

* **Insert** puts the cell in its leaf; a node that no longer fits is split by byte size into two (or, with very large cells, more) pages and the divider cells go to the parent, recursively. The root keeps its page number: when it splits, its contents move to new pages and it becomes an interior page. An insert at the end of the rightmost leaf (rowids in ascending order) puts only the new cell on the new page, so a sequential load fills pages.
* **Delete** removes the cell and frees its overflow pages. A non-root page left without cells is merged with a neighbour (and the parent loses a divider; for an index the parent's divider entry goes down into the merged page); if the two do not fit one page the cells are shared out again. A root left with a single child takes the child's contents. An index entry on an interior page is replaced by its predecessor (the last entry to its left), and the leaf copy removed. Pages freed go to the freelist and are reused before the file grows.
* **Invariants**: no non-root page is empty; all leaves of a tree are at the same depth; rowids and entries are strictly increasing and within the bounds the parents give. `integrity_check` verifies them.

## Transactions and crash recovery

Transactions are **no-steal**: every page a transaction changes stays in memory (a map from page number to bytes) until commit, so the file does not change before. Reads see the transaction's pages first. A savepoint is a copy of that map (pages are shared, so it is cheap) plus the header. The undo of one failed statement inside a transaction is cheaper still: while a statement runs the pager logs, for each page it changes, what the page held before (and the header and size); a failure puts those back, a success drops the log.

`commit` does, in this order, and `sync`s at the points named:

1. Writes page 1's header fields (change counter + 1, page count, freelist, schema cookie, `user_version`, version-valid-for).
2. Writes the **journal** `<file>-journal`: a 512-byte header (8 magic bytes `d9 d5 05 f9 20 a1 63 d7`, record count, a nonce, the size of the database in pages before the transaction, sector size 512, page size), then for every changed page that existed before: its number, its **original** bytes, and a checksum (the nonce plus every 200th byte from the end of the page). `sync` the journal.
3. Writes the changed pages into the database file. `sync` the file.
4. Deletes the journal. This is the commit point.

A crash before step 4 leaves a journal, a **hot journal**. The next `open_rw` (which holds the exclusive lock) reads it, writes the original pages back for every record whose checksum is valid (a torn tail is ignored: those pages were not yet changed), cuts the file to its old size, syncs, and deletes the journal. A journal whose header magic is wrong was never completed and is deleted. So a transaction is entirely there or not there. The format is SQLite's, so the stock `sqlite3` also recovers a database this crate crashed on, and this crate recovers one `sqlite3` crashed on.

A reader (`Database::open`) does not recover and refuses a file with a hot journal (`Unsupported`); it takes no lock, so a reader that overlaps a commit can see a mix of old and new pages. Open for writing when that matters.

## Locking

`open_rw` and `create` take an exclusive advisory lock on the database file (`File::try_lock`); a second writer gets `Error::Busy`. The lock lasts as long as the handle. There is no waiting and no retry. A process that forks while a database is open lends the lock to the child until it calls `exec`.

## The executor

* **Catalog**: built from the schema rows when a statement needs it, thrown away after any DDL and after a rollback. A table whose `CREATE` text uses syntax this crate does not run is "broken": using it gives `Unsupported`, the rest of the database stays usable.
* **A row** is the stored record with the `INTEGER PRIMARY KEY` column filled from the rowid, a column the record is too short for (added by `ADD COLUMN`) filled from its default, and a `REAL` column holding an integer turned into a real.
* **Reads** of a tree page do not copy its cells: a view of the page keeps where each cell lies, a descent binary-searches those, and only the page that is about to change is turned into a list of cells. A unique-key check or a key lookup therefore costs a few comparisons per level.
* **SELECT** builds its sources (tables, subqueries), picks an access path for each, loops over them as nested loops (the inner table by key when it can), filters with `WHERE`, groups and aggregates, projects, removes duplicates, sorts and cuts with `LIMIT`/`OFFSET`. Rows come off a tree in batches of up to 256 (the first batch is as small as `LIMIT` allows), so `LIMIT 5` of a big table reads a few pages. Everything after the scan is in memory.
* **Writes** read the rows they will change first and apply the changes after, so a statement never sees its own writes. A change goes through, in order: affinity, `NOT NULL`, `CHECK`, the conflict check against the rowid and every unique index, then the table and its indexes.
* **Grouping** orders groups by their keys (as `sqlite3` does); bare columns in an aggregate query take the first row of the group.

## Limits

| What | Limit |
|---|---|
| Database size when writing | under 1 GiB (the page at the lock byte is not handled); bigger files can be read |
| Changed pages per transaction | all in memory |
| Result of a query | all in memory |
| Depth of a tree | 64 (a deeper one is reported as a loop) |
| Page size | 512 to 65536, a power of two |
| Writers | one per file, one process |
| Recursion in the parser | by the depth of the SQL text; very deeply nested expressions use stack |
