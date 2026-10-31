---
title: Testing
parent: quench-sqlite
grand_parent: Crates
nav_order: 6
---

```text
cargo test -p quench-sqlite
```

The suites run in the crate's own `src` (the unit harness of [quench-test](../quench-test/index.md)). Several compare with the **stock `sqlite3`** and use it when it is on the `PATH`; without it those comparisons are skipped (the engine still runs the scripts to check that nothing fails) and the rest runs in full.

| File | What it checks |
|---|---|
| `tests.rs` | reading: varints and records round trip at every width, a database made by `sqlite3` (`tests/fixtures/sample.db`: 1200 rows, overflow blobs, two indexes) read row by row, `get_row`, index order, files that are not databases, UTF-16 and WAL refused, a page overwritten with garbage and a looping root give errors, not panics |
| `write_tests.rs` | writing: a new file passes `sqlite3`'s `integrity_check` at every page size; 20,000 ascending and descending inserts; random insert/replace/delete against a model at page sizes 512, 1024 and 4096; deleting everything frees the pages and refilling reuses them; dropping a table; an index agrees with its table for `sqlite3`; deleting an index down to nothing and filling again; duplicates and failed writes leave the file untouched; one writer at a time; **a crash at every write step** (the commit is made to fail after the k-th write, including a torn half-write) leaves the old state or the new, never a mix, and the file recovers; a hot journal blocks readers and `sqlite3` can open it. Every database it makes also passes this crate's `integrity_check` |
| `sql_tests.rs` | the parser: precedence and shapes, parameter numbering, every statement form, errors with positions, what is refused as unsupported; **77 statements both we and `sqlite3` accept and 40 both refuse**; 20,000 random token strings never panic |
| `engine_tests.rs` | the engine against `sqlite3` on the shape of Forklift's manifest (600 rows, a composite primary key, a status index): about 45 queries (lookups, ranges, aggregates, grouping, joins, subqueries, compound selects, functions, NULL logic) compared line by line, and write scripts (updates, deletes, `OR REPLACE`, upserts); all affinities; constraint errors; transactions and savepoints; parameters, `RETURNING`, `ADD COLUMN`, pragmas; **page counts**: a primary-key lookup in a 12,000-row table reads at most 6 pages, a unique-index lookup 12, an index equality less than a fifth of the table, `LIMIT 5` few, a full scan the table |
| `check_tests.rs` | `integrity_check`: sound files pass (from `sqlite3` and from here), an index that disagrees with its table is named, a page nobody uses and a wrong freelist count are named, and 150 random byte changes of a database are reported or refused and then queried and written to without a panic |

## How a crash is simulated

The pager has a counter, `fail_at`, set only by tests. The k-th write or sync of a commit fails (a write fails after half its bytes, as a power cut can leave it) and every later one fails too; the handle is then dropped as a crash would leave it. The test reopens the file (which recovers it) and compares the rows with the state before and after.

## Testing code that uses it

A database is a file: tests make one in a temporary directory (`quench_core::tmp::TempDir`) and drop it with the directory. There is no in-memory mode. `PRAGMA integrity_check` (or `Database::integrity_check`) is a cheap assertion at the end of a test that writes.
