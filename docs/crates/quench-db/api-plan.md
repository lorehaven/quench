---
title: API - plan
parent: quench-db
grand_parent: Crates
nav_order: 6
---

Module `quench_db::plan` (`src/plan.rs`): turns "install these modules" into an ordered, rendered, checksummed list of migrations. No database is touched.

## `struct InstallRequest`

```rust
#[derive(Debug, Clone, Default)]
pub struct InstallRequest {
    pub module: String,
    pub version: Option<Version>,           // default: the catalog's version
    pub schema: Option<String>,             // default: the module's fallback_schema()
    pub variables: BTreeMap<String, String>, // override the manifest's `variables`
}

pub fn new(module: impl Into<String>) -> Self
pub fn parse(spec: &str) -> Result<Self, PlanError>
```

`new` sets only `module`. `parse` accepts `module[@version][:schema]` (for example `sage@0.1.9:sage_test`): split at the first `:` (schema, trimmed, empty means none), then the head at the first `@` (version, trimmed, parsed with `Version::parse`). Errors: `PlanError::BadInstallSpec` with reason `missing module name` or the version parse error. `variables` is empty after `parse`.

## `struct ResolvedModule`

```rust
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub module: String,
    pub version: Version,
    pub schema: String,
    pub scope: ModuleScope,
    pub required_by: Option<String>,   // Some(dependent) when pulled in as a dependency
    pub variables: BTreeMap<String, String>,
}

#[must_use] pub fn instance(&self) -> String
```

One module installed into one schema. `instance()` is the ledger identity: `"{module}@{schema}"` for `Schema` scope, `"{module}@*"` for `Database` scope (one instance however many schemas ask for it).

## `struct PlannedMigration`

```rust
#[derive(Debug, Clone)]
pub struct PlannedMigration {
    pub id: String,               // ledger primary key: "{instance}:{migration_id}", e.g. "app@app:0001-tables"
    pub module: String,
    pub schema: String,
    pub migration_id: String,
    pub author: String,
    pub since: Version,           // migration_since(migration)
    pub module_version: Version,  // version the module is installed at
    pub statements: Vec<String>,  // rendered SQL, one per ChangeSet
    pub checksum: String,         // lowercase hex SHA-256, see below
}
```

`checksum` = SHA-256 over, for each statement in order, the statement bytes followed by `;\n`; hex-encoded with `quench_core::hex::encode`.

## `struct MigrationPlan`

```rust
#[derive(Debug, Clone, Default)]
pub struct MigrationPlan {
    pub modules: Vec<ResolvedModule>,       // dependency order
    pub migrations: Vec<PlannedMigration>,  // application order
}

#[must_use] pub const fn is_empty(&self) -> bool     // migrations.is_empty()
pub fn resolve(catalog: &Catalog, requests: &[InstallRequest]) -> Result<Self, PlanError>
```

`resolve` algorithm:

1. `requests` empty: `NothingToInstall`.
2. Each request: module must exist (`PlanError::Catalog(UnknownModule { required_by: None })`); version defaults to the catalog's, and a version above the catalog's is `VersionNotInCatalog`; schema defaults to `fallback_schema()`. The instance key (`module@schema` or `module@*`) must not already exist at a different version (`ConflictingVersions`); a repeated key at the same version replaces the earlier request (later request's `variables` win).
3. Requirements are followed (stack order) from every instance: the dependency's schema is its `fallback_schema()` if it is database-scoped, otherwise the requirement's `schema`, otherwise the dependent's schema. A dependency not already present is added at the **catalog** version with `required_by = Some(dependent)`; one already present (explicit or earlier) is kept as is.
4. Every recorded edge is then checked: the dependency instance's version must satisfy the requirement (`PlanError::Catalog(UnsatisfiedRequirement)`). This is how pinning an old version of a dependency is rejected.
5. Topological order (Kahn's algorithm; ties broken by instance key, so output is reproducible). Leftover nodes: `DependencyCycle { modules }` (sorted instance keys). Self-dependencies are ignored.
6. For each module, in order: variables = manifest `variables`, overridden by the request's `variables`, then the built-ins `schema`, `module`, `version` (built-ins win over user values with the same name). Each migration with `since <= version` (`migrations_up_to`) is rendered (`RenderError` becomes `PlanError::Render`), hashed, and appended.

### Variables

Built-in `${schema}`, `${module}`, `${version}` (the installed version, not the catalog's); plus manifest `[variables]`; plus `InstallRequest::variables`. Dependencies pulled in implicitly get no request variables.

## `enum PlanError`

```rust
#[derive(Debug, quench_core::Error)]
pub enum PlanError {
    NothingToInstall,
    BadInstallSpec { spec: String, reason: String },
    Catalog(#[from] CatalogError),
    VersionNotInCatalog { module: String, wanted: String, available: String },
    ConflictingVersions { module: String, schema: String, first: String, second: String },
    DependencyCycle { modules: Vec<String> },
    Render { module: String, migration: String, #[source] source: RenderError },
}
```

Texts in [errors](errors.md#planerror). `From<CatalogError>` exists.
