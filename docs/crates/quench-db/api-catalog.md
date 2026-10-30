---
title: API - catalog
parent: quench-db
grand_parent: Crates
nav_order: 4
---

Module `quench_db::catalog` (`src/catalog.rs`): a directory of independently versioned modules.

```text
migrations/
  quench-auth/
    module.toml            # name, version, requires, defaults
    0001-users.toml        # migrations, each tagged with `since`
  sage/
    module.toml
```

A module is installed into a target schema; the same module can be installed into several schemas of one database.

## `const MODULE_MANIFEST`

```rust
pub const MODULE_MANIFEST: &str = "module.toml";
```

A directory is a module only if it contains this file (as a file). Other directories and files directly under the root are ignored.

## `enum ModuleScope`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleScope {
    #[default] Schema,   // "schema": installed once per target schema
    Database,            // "database": installed once per database (extensions, roles)
}
```

TOML values: `scope = "schema"` or `scope = "database"`.

## `struct Requirement`

```rust
#[derive(Debug, Clone)]
pub struct Requirement {
    pub module: String,
    pub version: VersionReq,          // quench_core::version::VersionReq
    pub schema: Option<String>,
}
```

Deserialised (custom `Deserialize` impl) from either form:

```toml
requires = [
  "auth@^0.1",                                              # short: module[@req]; no @ means "*"
  { module = "pgvector", version = "^0.1", schema = "public" },  # table: version defaults to "*", schema optional
]
```

Short form: text before the first `@` is the module (trimmed; empty is `BadRequirement { reason: "missing module name" }`), text after is a `VersionReq` (`*`, `x`, `X`, or comma-separated comparators with `>=`, `<=`, `>`, `<`, `=`, `~`, `^` or none). A parse failure is `CatalogError::BadRequirement` for the short form and a `quench_serde::Error` (reported as `BadManifest`) for the table form. A TOML value that is neither string nor table: error "a requirement string or table".

Without `schema`, a dependency is installed into the schema of the module that requires it (schema-scoped dependencies) — see [plan](api-plan.md). `schema` is ignored for database-scoped dependencies.

## `struct ModuleManifest`

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleManifest {
    pub name: String,
    pub version: Version,                       // quench_core::version::Version
    #[serde(default)] pub description: Option<String>,
    #[serde(default)] pub scope: ModuleScope,
    #[serde(default)] pub default_schema: Option<String>,
    #[serde(default)] pub requires: Vec<Requirement>,
    #[serde(default)] pub variables: BTreeMap<String, String>,
}
```

Contents of `module.toml`. `name` and `version` are required. `variables` are the default `${var}` values for the module's migrations (see [plan](api-plan.md#variables)).

```rust
#[must_use] pub fn fallback_schema(&self) -> String
```

`default_schema` if set; otherwise the module `name` for `Schema` scope, `"public"` for `Database` scope.

## `struct CatalogModule`

```rust
#[derive(Debug, Clone)]
pub struct CatalogModule {
    pub manifest: ModuleManifest,
    pub migrations: Vec<Migration>,
    pub dir: PathBuf,
}
```

Methods: `#[must_use] pub fn name(&self) -> &str`; `#[must_use] pub const fn version(&self) -> &Version`; `#[must_use] pub fn migrations_up_to(&self, version: &Version) -> Vec<&Migration>` (those with `migration_since(m) <= version`, in catalog order).

## `fn migration_since`

```rust
#[must_use] pub fn migration_since(migration: &Migration) -> Version
```

`migration.since` parsed as a `Version`; `0.0.0` when it is `None` **or does not parse** (a bad `since` is caught earlier by `Catalog::load`, which rejects it).

## `struct Catalog`

```rust
#[derive(Debug, Clone, Default)]
pub struct Catalog { /* modules: BTreeMap<String, CatalogModule> */ }
```

```rust
pub fn load<P: AsRef<Path>>(root: P) -> Result<Self, CatalogError>
#[must_use] pub fn get(&self, name: &str) -> Option<&CatalogModule>
pub fn modules(&self) -> impl Iterator<Item = &CatalogModule>      // name order
#[must_use] pub fn len(&self) -> usize
#[must_use] pub fn is_empty(&self) -> bool
```

`Catalog::default()` is an empty catalog. `load` does, in order:

1. `root` must be a directory, else `MissingCatalog`.
2. Reads the root's entries (`Io` on failure), keeps sub-directories containing `module.toml`, sorts by path.
3. For each: parses `module.toml` (`Io` / `BadManifest`), loads its migrations with `MigrationLoader::load_from_dir_excluding(dir, ["module.toml"])` (`BadMigrations`). Two directories with the same module `name` give `DuplicateModule`.
4. Validates (modules in name order): duplicate migration id in a module (`DuplicateMigrationId`); `since` that does not parse (`BadMigrationVersion`); `since` greater than the module version (`MigrationAheadOfModule`); each `requires` entry names an existing module (`UnknownModule`) whose catalog version satisfies the requirement (`UnsatisfiedRequirement`).

The first error wins. Dependency cycles are not checked here; they surface in `MigrationPlan::resolve` (`DependencyCycle`).

## `enum CatalogError`

```rust
#[derive(Debug, quench_core::Error)]
pub enum CatalogError {
    MissingCatalog { path: PathBuf },
    Io { path: PathBuf, source: std::io::Error },
    BadManifest { path: PathBuf, reason: String },
    BadMigrations { module: String, reason: String },
    BadRequirement { spec: String, reason: String },
    DuplicateModule { name: String, first: PathBuf, second: PathBuf },
    DuplicateMigrationId { module: String, id: String },
    BadMigrationVersion { module: String, id: String, reason: String },
    MigrationAheadOfModule { module: String, id: String, since: String, module_version: String },
    UnknownModule { name: String, required_by: Option<String> },
    UnsatisfiedRequirement { module: String, required_by: String, wanted: String, available: String },
}
```

Display texts are in [errors](errors.md#catalogerror). `Io.source` is a plain field, not marked `#[source]`.
