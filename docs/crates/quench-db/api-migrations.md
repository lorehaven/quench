---
title: API - migration files
parent: quench-db
grand_parent: Crates
nav_order: 5
---

Module `quench_db::migrations` (`src/migrations.rs`): the data model of a migration, the on-disk TOML format, `${var}` rendering and the directory loader.

## `struct Migration`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub id: String,
    pub author: String,
    #[serde(default)]
    pub since: Option<String>,
    pub changes: Vec<ChangeSet>,
}
```

* `id`: unique within a module; by convention `NNNN-name`. The loader sorts migrations of a file by `id` (string order) and requires the text before the first `-` to be unique inside one file.
* `author`: stored in the ledger.
* `since`: module version in which the migration was introduced. A migration is planned only when `since <= version being installed`. `None` (or text that does not parse as a version, see [catalog](api-catalog.md#migration_since)) counts as `0.0.0`, always applied.
* `changes`: executed in order.

`impl Migration`:

```rust
pub fn to_sql_with(&self, vars: &BTreeMap<String, String>) -> Result<Vec<String>, RenderError>
```

Renders each change with `ChangeSet::to_sql_with`; fails on the first error.

## `fn render`

```rust
pub fn render(template: &str, vars: &BTreeMap<String, String>) -> Result<String, RenderError>
```

Replaces every `${name}` with `vars[name]`. The name is trimmed (`${ schema }` is `schema`). Substituted values are **not** rescanned. Unknown names are an error; `${` without a closing `}` is an error. A lone `$` or `}` is left alone. No escape syntax exists for a literal `${`.

## `enum RenderError`

```rust
#[derive(Debug, quench_core::Error)]
pub enum RenderError {
    #[error("unknown variable '{name}' (known: {})", known.join(", "))]
    UnknownVariable { name: String, known: Vec<String> },
    #[error("unterminated '${{' in template: {template}")]
    Unterminated { template: String },
}
```

`known` is the sorted key list of the variable map.

## `struct MigrationFile`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFile { pub migrations: Vec<Migration> }
```

The shape of one `*.toml` migration file: an array of tables `[[migrations]]`.

## `struct MigrationLoader`

```rust
#[derive(Debug)] pub struct MigrationLoader;

impl MigrationLoader {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> quench_core::Result<Vec<Migration>>;
    pub fn load_from_dir_excluding<P: AsRef<Path>>(dir: P, exclude: &[&str]) -> quench_core::Result<Vec<Migration>>;
}
```

`load_from_dir` is `load_from_dir_excluding(dir, &[])`. Behaviour:

1. Lists `dir` non-recursively; keeps entries whose extension is `toml` and whose file name is not in `exclude`. A missing or unreadable directory yields an empty list (no error); unreadable entries are skipped silently.
2. Sorts files by file name.
3. Reads each (read errors are returned), parses it as `MigrationFile` (a parse error becomes `"{path}: {error}"`).
4. Sorts the file's migrations by `id` and fails with `Duplicate migration prefix '{prefix}' in file {path}` if two share the part of `id` before the first `-`.
5. Concatenates the results file by file.

Duplicate ids **across** files are not detected here (the [catalog](api-catalog.md) detects them).

## `enum ChangeSet`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeSet {
    #[serde(rename = "sql")]             Sql(String),
    #[serde(rename = "createSchema")]    CreateSchema { name: String },
    #[serde(rename = "createExtension")] CreateExtension { name: String },
    #[serde(rename = "createTable")]     CreateTable { schema: Option<String>, name: String, columns: Vec<ColumnDef> },
}
```

TOML spelling (externally tagged, one key per `[[migrations.changes]]` table):

```toml
[[migrations.changes]]
sql = "ALTER TABLE ${schema}.t ADD COLUMN note TEXT"

[[migrations.changes]]
createSchema = { name = "${schema}" }

[[migrations.changes]]
createExtension = { name = "vector" }

[[migrations.changes]]
createTable = { schema = "${schema}", name = "t", columns = [
  { name = "id", data_type = "TEXT", primary_key = true, nullable = false },
] }
```

### `to_sql`

```rust
#[must_use] pub fn to_sql(&self) -> String
pub fn to_sql_with(&self, vars: &BTreeMap<String, String>) -> Result<String, RenderError>
```

`to_sql_with` is `render(&self.to_sql(), vars)`. Generated text:

| Variant | SQL |
|---|---|
| `Sql(s)` | `s` unchanged |
| `CreateSchema { name }` | `CREATE SCHEMA IF NOT EXISTS {name}` |
| `CreateExtension { name }` | `CREATE EXTENSION IF NOT EXISTS {name}` |
| `CreateTable { schema, name, columns }` | `CREATE TABLE IF NOT EXISTS {schema.}{name} ({defs})` where each def is `{name} {data_type}`, plus ` PRIMARY KEY` if `primary_key == Some(true)`, plus ` NOT NULL` if `nullable == Some(false)`; defs joined by `, ` |

Identifiers are inserted verbatim (no quoting, no validation). Because rendering happens after generation, `${var}` works inside every field.

## `struct ColumnDef`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub primary_key: Option<bool>,   // None = false
    pub nullable: Option<bool>,      // None = true
}
```

*Unverified:* whether an omitted `schema`, `primary_key` or `nullable` key in TOML deserialises as `None`; no test in the crate covers an omitted optional key of a `createTable` change (the `since` field has `#[serde(default)]`, the others do not).
