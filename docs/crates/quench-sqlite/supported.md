---
title: Supported SQL
parent: quench-sqlite
grand_parent: Crates
nav_order: 3
---

The dialect is SQLite's. What follows is what runs; anything else is refused with `Error::Unsupported` (when the syntax is understood) or `Error::Syntax`.

## Statements

| Statement | Supported | Not supported |
|---|---|---|
| `SELECT` | `DISTINCT`/`ALL`; `*`, `t.*`, expressions with aliases; `FROM` with tables, aliases, subqueries, `,`, `[INNER] JOIN`, `LEFT [OUTER] JOIN`, `CROSS JOIN`, `ON`, `USING`; `WHERE`; `GROUP BY` (expressions, ordinals, aliases); `HAVING`; `UNION`, `UNION ALL`, `INTERSECT`, `EXCEPT`; `ORDER BY` (expressions, ordinals, aliases, `ASC`/`DESC`, `NULLS FIRST`/`LAST`, `COLLATE`); `LIMIT n [OFFSET m]` and `LIMIT m, n` | `WITH` (CTEs), window functions, `VALUES` as a query, `RIGHT`/`FULL`/`NATURAL` joins, table-valued functions, row values |
| `INSERT` | `VALUES` (several rows), `SELECT`, `DEFAULT VALUES`, column lists, `OR REPLACE/IGNORE/ABORT/FAIL/ROLLBACK`, `REPLACE INTO`, `ON CONFLICT [(cols)] DO NOTHING` and `DO UPDATE SET ... [WHERE ...]` (with `excluded.col`), `RETURNING` | a `WHERE` on the conflict target (partial indexes) |
| `UPDATE` | `SET col = expr, ...`, `WHERE`, `OR ...`, `RETURNING`; changing the rowid | `UPDATE ... FROM`, `(a, b) = (...)`, `ORDER BY`/`LIMIT` |
| `DELETE` | `WHERE`, `RETURNING` | `ORDER BY`/`LIMIT` |
| `CREATE TABLE` | `IF NOT EXISTS`; types (any text, affinity by SQLite's rules); `PRIMARY KEY` (column or table level; `INTEGER PRIMARY KEY` is the rowid), `NOT NULL`, `UNIQUE`, `CHECK`, `DEFAULT` (literal, signed number, parenthesised expression), `COLLATE` (BINARY, NOCASE, RTRIM) | `AUTOINCREMENT`, `WITHOUT ROWID`, `AS SELECT`, foreign keys (`REFERENCES`, `FOREIGN KEY`), generated columns, conflict clauses on constraints, `TEMP` tables |
| `CREATE INDEX` | `[UNIQUE]`, `IF NOT EXISTS`, several columns; an existing table's rows are indexed | `DESC` columns, expressions, partial indexes (`WHERE`), a column with a collation other than BINARY |
| `DROP TABLE`, `DROP INDEX` | `IF EXISTS`; dropping a table drops its indexes; an automatic index cannot be dropped | views, triggers |
| `ALTER TABLE` | `ADD COLUMN` (no `PRIMARY KEY`/`UNIQUE`; `NOT NULL` needs a `DEFAULT`; the default is a constant) | `RENAME`, `DROP COLUMN` |
| `BEGIN [DEFERRED/IMMEDIATE/EXCLUSIVE]`, `COMMIT`/`END`, `ROLLBACK`, `SAVEPOINT`, `RELEASE`, `ROLLBACK TO` | all; savepoints only inside a `BEGIN` | `SAVEPOINT` outside a transaction |
| `PRAGMA` | see [the API](api.md#pragmas) | the rest |
| Not parsed | `CREATE VIEW`, `CREATE TRIGGER`, `CREATE VIRTUAL TABLE`, `ATTACH`, `VACUUM`, `ANALYZE`, `REINDEX`, `EXPLAIN` | |

`main.t` is `t`; any other schema name is refused.

## Expressions

Literals (integers, reals, `0x` hex, strings, `x'..'` blobs, `NULL`, `TRUE`/`FALSE`), parameters, columns (`c`, `t.c`, `rowid`/`_rowid_`/`oid`), `+ - * / %`, `|| & | << >> ~`, unary `+ -`, comparisons, `IS [NOT]`, `IS [NOT] DISTINCT FROM`, `[NOT] IN (list)` and `IN (subquery)`, `[NOT] BETWEEN`, `[NOT] LIKE ... [ESCAPE]`, `[NOT] GLOB`, `ISNULL`/`NOTNULL`/`NOT NULL`, `AND`/`OR`/`NOT` with three-valued logic, `CASE`, `CAST`, `COLLATE`, scalar subqueries, `EXISTS`, correlated subqueries. In the order of SQLite's precedence. Not supported: `REGEXP`, `MATCH`, `->` and `->>`.

### Functions

| Kind | Functions |
|---|---|
| Aggregate | `count(*)`, `count(x)`, `count(DISTINCT x)`, `sum`, `total`, `avg`, `min`, `max`, `group_concat(x [, sep])` (alias `string_agg`) |
| Scalar | `abs`, `length`, `lower`, `upper` (ASCII only), `typeof`, `coalesce`, `ifnull`, `nullif`, `iif`, `min(a, b, ...)`, `max(a, b, ...)`, `substr`/`substring`, `trim`, `ltrim`, `rtrim`, `replace`, `instr`, `round`, `hex`, `quote` |

Other functions (dates and times, `printf`, `random`, `json_*`, math functions) give `Error::Unsupported` naming the function.

## Types and comparisons

* Column affinity follows SQLite's five rules (`INT` makes INTEGER, `CHAR`/`CLOB`/`TEXT` TEXT, `BLOB` or no type none, `REAL`/`FLOA`/`DOUB` REAL, anything else NUMERIC) and is applied when a value is stored. A `REAL` column stores reals.
* A comparison applies the numeric affinity of one side to the other when that is text (`WHERE int_col = '5'` matches 5).
* Integer arithmetic that overflows becomes a real; division by zero is NULL; `/` of two integers is an integer division.
* A real prints with 15 significant digits when those read back as the same number, else 17 (the way SQLite 3.4x prints; the last digit of a 17-digit value can differ from SQLite's own routine).

## Constraint errors

`UNIQUE constraint failed: t.a, t.b`, `NOT NULL constraint failed: t.c`, `CHECK constraint failed: t`: [`Error::Constraint`](errors.md). A `CHECK` that evaluates to NULL passes.

## Access paths

The planner uses the rowid, a unique or plain index, in this order of preference: rowid equality; an index with the longest run of leading `=` columns (a complete unique match first), with a range on the next column; a rowid range (`<`, `<=`, `>`, `>=`, `BETWEEN`); a full scan. Tables are joined in the order of `FROM`, the inner one by key when the `ON`/`WHERE` terms allow. `WHERE` is always evaluated in full on the rows an access path returns. `ORDER BY` sorts in memory.
