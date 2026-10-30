---
title: API - runner
parent: quench-db
grand_parent: Crates
nav_order: 7
---

Module `quench_db::runner` (`src/runner.rs`, feature `postgres`): applies a [`MigrationPlan`](api-plan.md) to Postgres and records what was done in a ledger.

State lives in a dedicated schema (`foundry` by default) with two tables. A session-level advisory lock prevents concurrent runs (two Kubernetes Jobs, a Job racing an init container) from applying the same plan twice.

## Constants

```rust
pub const DEFAULT_LEDGER_SCHEMA: &str = "foundry";
pub const DEFAULT_LEDGER_TABLE: &str = "forge_migrations";
pub const DEFAULT_MODULE_TABLE: &str = "forge_modules";
```

## Ledger tables

Created by `apply` (not by dry runs) with `CREATE SCHEMA IF NOT EXISTS {ledger_schema}` and `CREATE TABLE IF NOT EXISTS`:

`{ledger_schema}.{ledger_table}` (one row per applied migration instance):

| Column | Type | Notes |
|---|---|---|
| `id` | `TEXT PRIMARY KEY` | `PlannedMigration::id` |
| `module` | `TEXT NOT NULL` | |
| `target_schema` | `TEXT NOT NULL` | |
| `migration_id` | `TEXT NOT NULL` | |
| `since` | `TEXT NOT NULL` | |
| `module_version` | `TEXT NOT NULL` | |
| `author` | `TEXT NOT NULL` | |
| `checksum` | `TEXT NOT NULL` | |
| `applied_at` | `TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP` | |

`{ledger_schema}.{module_table}` (one row per installed module instance):

| Column | Type | Notes |
|---|---|---|
| `module` | `TEXT NOT NULL` | part of the primary key |
| `target_schema` | `TEXT NOT NULL` | part of the primary key |
| `version` | `TEXT NOT NULL` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP` | |

`PRIMARY KEY (module, target_schema)`. Note that a database-scoped module is recorded with its own schema (usually `public`), not `*`.

## `enum MigrationOutcome`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationOutcome {
    Applied,   // statements were executed and recorded
    Skipped,   // already in the ledger
    Pending,   // would be applied (dry run only)
}
```

## `struct MigrationResult`

```rust
#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub id: String,
    pub module: String,
    pub schema: String,
    pub migration_id: String,
    pub outcome: MigrationOutcome,
}
```

## `struct ApplyReport`

```rust
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    pub results: Vec<MigrationResult>,   // plan order
    pub dry_run: bool,
}

#[must_use] pub fn count(&self, outcome: MigrationOutcome) -> usize
pub fn with_outcome(&self, outcome: MigrationOutcome) -> impl Iterator<Item = &MigrationResult>
```

## `struct ResetReport`

```rust
#[derive(Debug, Clone, Default)]
pub struct ResetReport {
    pub schemas: Vec<String>,            // schemas dropped (sorted)
    pub forgotten_migrations: usize,     // ledger rows deleted
}
```

## `struct MigrationRunner`

```rust
#[derive(Debug, Clone)]
pub struct MigrationRunner { /* ledger_schema, ledger_table, module_table, allow_drift, dry_run */ }
impl Default for MigrationRunner   // foundry / forge_migrations / forge_modules, allow_drift = false, dry_run = false
```

Builder methods (all `#[must_use]`, consuming `self`):

```rust
pub fn new() -> Self
pub fn ledger_schema(self, schema: impl Into<String>) -> Self
pub fn ledger_table(self, table: impl Into<String>) -> Self
pub fn module_table(self, table: impl Into<String>) -> Self
pub const fn allow_drift(self, allow: bool) -> Self
pub const fn dry_run(self, dry_run: bool) -> Self
```

`allow_drift`: accept migrations whose SQL changed after they were applied. `dry_run`: report what would happen without writing anything.

Identifier rule: the three names are interpolated into DDL, so `apply`, `reset` and `installed_modules` first require each to be non-empty, only ASCII letters, digits or `_`, and not starting with a digit; otherwise `DbError::Unknown("invalid {kind} '{value}': expected an unquoted identifier")` with kind `ledger schema`, `ledger table`, `module table` (or `target schema` for reset targets).

### `apply`

```rust
pub async fn apply(&self, db: &PostgresDb, plan: &MigrationPlan) -> Result<ApplyReport, DbError>
```

1. Validates identifiers.
2. Takes one pooled connection. Unless `dry_run`, runs `SELECT pg_advisory_lock($1)` (blocking) with the key `advisory_lock_key("{ledger_schema}.{ledger_table}")`.
3. Unless `dry_run`: creates the ledger schema and both tables.
4. Reads `id, checksum` from the ledger (empty map if the ledger table does not exist, via `to_regclass`).
5. For each planned migration, in order:
   * in the ledger with an equal checksum: `Skipped`;
   * in the ledger with a different checksum: `DbError::Unknown("migration '{id}' changed after it was applied (recorded {old}, catalog {new})")`, or with `allow_drift` a `warn!` log and `Skipped`;
   * not in the ledger, dry run: `Pending`;
   * otherwise: `BEGIN`; each statement through the simple protocol (a statement failure becomes `DbError::QueryError("migration '{id}': {error}")`); insert the ledger row (`ON CONFLICT (id) DO NOTHING`); `COMMIT`; outcome `Applied`.
6. Unless `dry_run`: upserts a row in the module table for **every** module in the plan (`ON CONFLICT (module, target_schema) DO UPDATE SET version, updated_at`).
7. Releases the lock (`pg_advisory_unlock`, result ignored) on both success and error paths, then returns the connection.

Failure semantics: each migration is its own transaction. When one fails, earlier ones stay applied and recorded, later ones are not attempted, and step 6 (module rows) is skipped.

### `reset`

```rust
pub async fn reset(&self, db: &PostgresDb, plan: &MigrationPlan) -> Result<ResetReport, DbError>
```

Development only. Always takes the advisory lock (it does **not** consult `dry_run`: a runner configured with `.dry_run(true)` still drops schemas). Then, for every schema from `resettable_schemas`: validates the name, logs `warn!("dropping schema {schema}")`, runs `DROP SCHEMA IF EXISTS {schema} CASCADE`. Then deletes from the ledger table `WHERE id = ANY($1)` the ids of all planned migrations (if the table exists), and from the module table the `(module, target_schema)` rows of the plan's modules (if it exists). Database-scoped objects (extensions) are not dropped, but their ledger rows are forgotten.

### `resettable_schemas`

```rust
#[must_use] pub fn resettable_schemas(&self, plan: &MigrationPlan) -> Vec<String>
```

The schemas `reset` would drop: those of the plan's schema-scoped modules, excluding `public` and the ledger schema; sorted, de-duplicated. Lets a CLI show the same list it will act on.

### `installed_modules`

```rust
pub async fn installed_modules(&self, db: &PostgresDb) -> Result<Vec<(String, String, String)>, DbError>
```

`(module, target_schema, version)` rows of the module table, `ORDER BY 1, 2`; an empty list if the table does not exist.

## Hidden public helpers

Marked `#[doc(hidden)]` but `pub` (used by tests):

```rust
pub fn validate_identifier(kind: &str, value: &str) -> Result<(), DbError>
#[must_use] pub fn advisory_lock_key(name: &str) -> i64
```

`advisory_lock_key` is the first eight bytes of `sha256(name)` read as a big-endian `i64`.
