---
title: Internals
parent: quench-db
grand_parent: Crates
nav_order: 10
---

Design notes that matter to users.

## Backend differences

`Repository` hides the backend, but the two behave differently in edge cases (taken from the code in `src/backends/`):

| Case | Postgres | In-memory |
|---|---|---|
| `create` with an existing id | the table's constraints decide: `QueryError` on a primary-key violation | silently replaces the row |
| `update` of a missing row | `NotFound` | inserts it |
| `find_by` on a non-text column | `column = $1` with the text value cast by the server to the column type (`"5"` matches integer 5) | never matches (only JSON strings compare) |
| `list` order | whatever the server returns | `HashMap` order |
| primary key | any type the table uses | JSON string or `i64` only; an unsigned value above `i64::MAX` fails `create` with `Unknown("Primary key must be string or number")` |
| `execute`, `migrate` | real SQL | no-ops |
| unknown table | server error | an empty table is created on use |
| defaults/constraints/types | the table's | none; any serialisable value is stored |

## Identifier handling

Table, primary-key and `columns()` names come from the `Model` implementation and are interpolated into SQL without quoting or validation. Values are always bound parameters. The only identifier taken from a caller at run time, the `find_by` column, is allow-listed against `T::columns()`. The runner's ledger names are validated as plain identifiers; `DB_MIGRATION_TABLE` is not.

## Mapping through JSON

`create` and `update` serialise the model to a `quench_serde::Value` and send it as one `jsonb`-typed parameter to `jsonb_populate_record(NULL::table, $1)`, so the server maps field names to columns and converts types. Reads go the other way: `quench_pg::Row::deserialize` turns a row into an object keyed by column name (types converted by OID) and deserialises the model from it. *Unverified:* what happens to columns that have no matching model field; that is decided by the `Deserialize` implementation of the model.

## Migration identity and drift

A planned migration is identified by `module@schema:migration-id` (`module@*:` for database-scoped modules) and verified by a SHA-256 checksum of its rendered statements. Rendering includes the variables, so editing the SQL, or a manifest variable that the SQL uses, after the migration was applied changes the checksum and counts as drift. Installing the same module into another schema is a different instance with its own ledger row.

## Locking

`apply` and `reset` hold a session advisory lock (`pg_advisory_lock`, blocking) keyed by the first eight bytes of `sha256("{ledger_schema}.{ledger_table}")`. The lock lives on one pooled connection for the whole run and is released explicitly. Two runners with different ledger tables use different keys and do not exclude each other. `Database::migrate` takes no lock.

## Order of application

Modules in dependency order (ties by instance key `module@schema`); within a module, files in file-name order, migrations in id order within a file. The plan is byte-for-byte reproducible for the same catalog and requests.

## Third-party code

Only the allowed set: see the [dependency table](index.md#dependencies). `ring` appears through `quench-crypto` (hashing).
