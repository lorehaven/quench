---
title: Errors
parent: quench-db
grand_parent: Crates
nav_order: 9
---

`quench-db` has four error types: `DbError` (CRUD, backends, runner), `CatalogError` (loading a catalog), `PlanError` (resolving a plan) and `RenderError` (substituting `${var}`). All derive `quench_core::Error` (a `thiserror`-style derive); none is `Clone`.

## `DbError`

| Variant | Display | Produced by |
|---|---|---|
| `ConnectionError(String)` | `Database connection error: {0}` | `Db::connect` without the `postgres` feature and a non-empty URL; `quench_pg` errors `Io`, `Closed`, `Timeout`, `Tls`, `Auth`, `Config` |
| `QueryError(String)` | `Query execution error: {0}` | every other `quench_pg::Error` (`Server`, `Protocol`, `RowCount` with a count other than 0); a failing statement inside a runner migration (`migration '{id}': {error}`) |
| `SerializationError(String)` | `Serialization error: {0}` | `quench_pg::Error::Decode`; any `quench_serde::Error` (`From`) |
| `NotFound` | `Record not found` | `quench_pg::Error::RowCount { got: 0, .. }`, i.e. Postgres `update` of a missing row or any other single-row query that returned nothing |
| `Unknown(String)` | `Unknown database error: {0}` | unknown `find_by` column; missing/invalid primary key; invalid runner identifier; migration drift; in-memory key problems |

Mapping from `quench_pg::Error` (`src/error.rs`):

```rust
match err {
    Error::RowCount { got: 0, .. } => NotFound,
    Error::Io(_) | Closed | Timeout(_) | Tls(_) | Auth(_) | Config(_) => ConnectionError(err.to_string()),
    Error::Decode(_) => SerializationError(err.to_string()),
    _ => QueryError(err.to_string()),
}
```

The mapping loses the structure: a unique violation arrives as `QueryError` with the text `ERROR: duplicate key value violates unique constraint "..." (SQLSTATE 23505): ...`; the SQLSTATE can only be recovered by matching on the text. A statement timeout is `ConnectionError` (it is `Error::Timeout`, and the connection is discarded).

## `CatalogError`

| Variant | Display |
|---|---|
| `MissingCatalog { path }` | `migration catalog not found at {path}` |
| `Io { path, source }` | `failed to read {path}: {source}` |
| `BadManifest { path, reason }` | `invalid module manifest {path}: {reason}` |
| `BadMigrations { module, reason }` | `invalid migrations in module '{module}': {reason}` |
| `BadRequirement { spec, reason }` | `invalid requirement '{spec}': {reason}` |
| `DuplicateModule { name, first, second }` | `module '{name}' defined twice: {first} and {second}` |
| `DuplicateMigrationId { module, id }` | `duplicate migration id '{id}' in module '{module}'` |
| `BadMigrationVersion { module, id, reason }` | `migration '{id}' in module '{module}' has an invalid `since`: {reason}` |
| `MigrationAheadOfModule { module, id, since, module_version }` | `migration '{id}' in module '{module}' is tagged since {since}, ahead of the module version {module_version}` |
| `UnknownModule { name, required_by }` | `unknown module '{name}'` plus ` required by '{m}'` when `required_by` is `Some` |
| `UnsatisfiedRequirement { module, required_by, wanted, available }` | `'{required_by}' requires {module} {wanted}, but the catalog provides {module} {available}` |

## `PlanError`

| Variant | Display |
|---|---|
| `NothingToInstall` | `no modules requested` |
| `BadInstallSpec { spec, reason }` | `invalid install spec '{spec}': {reason}` |
| `Catalog(CatalogError)` | the inner message (`{0}`) |
| `VersionNotInCatalog { module, wanted, available }` | `module '{module}' {wanted} requested, but the catalog only defines up to {available}` |
| `ConflictingVersions { module, schema, first, second }` | `module '{module}' requested twice for schema '{schema}' at {first} and {second}` |
| `DependencyCycle { modules }` | `dependency cycle between: {modules joined by ", "}` |
| `Render { module, migration, source }` | `failed to render migration '{migration}' of module '{module}': {source}` |

## `RenderError`

| Variant | Display |
|---|---|
| `UnknownVariable { name, known }` | `unknown variable '{name}' (known: {known joined by ", "})` |
| `Unterminated { template }` | `unterminated '${' in template: {template}` |

## Where each error type surfaces

* `Catalog::load`: `CatalogError`.
* `MigrationPlan::resolve`, `InstallRequest::parse`: `PlanError`.
* `MigrationLoader::*`: `quench_core::Result` (a `quench_core::Error`), not `CatalogError`; inside `Catalog::load` it becomes `BadMigrations`.
* `MigrationRunner::*`, `PostgresDb`, `Crud`, `Database`: `DbError`.
