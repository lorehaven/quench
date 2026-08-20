//! Unit tests for `migrations.rs`.

use quench_db::migrations::{ChangeSet, ColumnDef, Migration, MigrationFile, MigrationLoader};
use std::collections::BTreeMap;
use std::path::Path;

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn migration_to_sql_with_substitutes_every_change() {
    let migration = Migration {
        id: "0001-schema".to_string(),
        author: "a".to_string(),
        since: None,
        changes: vec![
            ChangeSet::CreateSchema {
                name: "${schema}".to_string(),
            },
            ChangeSet::Sql("SELECT 1".to_string()),
        ],
    };
    let statements = migration
        .to_sql_with(&vars(&[("schema", "sage")]))
        .expect("render");
    assert_eq!(
        statements,
        vec![
            "CREATE SCHEMA IF NOT EXISTS sage".to_string(),
            "SELECT 1".to_string(),
        ]
    );
}

#[test]
fn render_leaves_text_without_placeholders_untouched() {
    let out = quench_db::migrations::render("plain text", &BTreeMap::new()).expect("render");
    assert_eq!(out, "plain text");
}

#[test]
fn render_substitutes_a_known_placeholder() {
    let out = quench_db::migrations::render("hello ${name}!", &vars(&[("name", "world")]))
        .expect("render");
    assert_eq!(out, "hello world!");
}

#[test]
fn render_trims_whitespace_inside_the_placeholder() {
    let out = quench_db::migrations::render("${ name }", &vars(&[("name", "x")])).expect("render");
    assert_eq!(out, "x");
}

#[test]
fn render_errors_on_an_unknown_placeholder() {
    let err = quench_db::migrations::render("${missing}", &BTreeMap::new()).unwrap_err();
    assert!(err.to_string().contains("unknown variable 'missing'"));
}

#[test]
fn render_errors_on_an_unterminated_placeholder() {
    let err = quench_db::migrations::render("${oops", &BTreeMap::new()).unwrap_err();
    assert!(err.to_string().contains("unterminated"));
}

#[test]
fn change_set_sql_variant_passes_through_verbatim() {
    assert_eq!(ChangeSet::Sql("SELECT 1".to_string()).to_sql(), "SELECT 1");
}

#[test]
fn change_set_create_schema_and_extension_render_if_not_exists() {
    assert_eq!(
        ChangeSet::CreateSchema {
            name: "sage".to_string()
        }
        .to_sql(),
        "CREATE SCHEMA IF NOT EXISTS sage"
    );
    assert_eq!(
        ChangeSet::CreateExtension {
            name: "pgvector".to_string()
        }
        .to_sql(),
        "CREATE EXTENSION IF NOT EXISTS pgvector"
    );
}

#[test]
fn change_set_create_table_without_schema_uses_the_bare_name() {
    let sql = ChangeSet::CreateTable {
        schema: None,
        name: "users".to_string(),
        columns: vec![ColumnDef {
            name: "id".to_string(),
            data_type: "TEXT".to_string(),
            primary_key: Some(true),
            nullable: Some(false),
        }],
    }
    .to_sql();
    assert_eq!(
        sql,
        "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY NOT NULL)"
    );
}

#[test]
fn change_set_create_table_qualifies_the_name_with_its_schema() {
    let sql = ChangeSet::CreateTable {
        schema: Some("sage".to_string()),
        name: "users".to_string(),
        columns: vec![ColumnDef {
            name: "email".to_string(),
            data_type: "TEXT".to_string(),
            primary_key: None,
            nullable: None,
        }],
    }
    .to_sql();
    assert_eq!(sql, "CREATE TABLE IF NOT EXISTS sage.users (email TEXT)");
}

#[test]
fn change_set_to_sql_with_substitutes_placeholders_in_the_rendered_sql() {
    let sql = ChangeSet::CreateExtension {
        name: "${ext}".to_string(),
    }
    .to_sql_with(&vars(&[("ext", "pgvector")]))
    .expect("render");
    assert_eq!(sql, "CREATE EXTENSION IF NOT EXISTS pgvector");
}

fn write_migration_file(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("write migration file");
}

#[test]
fn migration_loader_loads_and_sorts_files_and_their_migrations() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    write_migration_file(
        temp.path(),
        "0002-second.toml",
        r#"
[[migrations]]
id = "0003-c"
author = "a"
changes = [{ sql = "SELECT 3" }]
"#,
    );
    write_migration_file(
        temp.path(),
        "0001-first.toml",
        r#"
[[migrations]]
id = "0002-b"
author = "a"
changes = [{ sql = "SELECT 2" }]

[[migrations]]
id = "0001-a"
author = "a"
changes = [{ sql = "SELECT 1" }]
"#,
    );

    let migrations = MigrationLoader::load_from_dir(temp.path()).expect("load");
    let ids: Vec<&str> = migrations.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["0001-a", "0002-b", "0003-c"]);
}

#[test]
fn migration_loader_excludes_named_files() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    write_migration_file(temp.path(), "module.toml", "name = \"x\"\n");
    write_migration_file(
        temp.path(),
        "0001-only.toml",
        r#"
[[migrations]]
id = "0001-a"
author = "a"
changes = []
"#,
    );

    let migrations =
        MigrationLoader::load_from_dir_excluding(temp.path(), &["module.toml"]).expect("load");
    assert_eq!(migrations.len(), 1);
}

#[test]
fn migration_loader_rejects_a_duplicate_prefix_within_one_file() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    write_migration_file(
        temp.path(),
        "0001-dup.toml",
        r#"
[[migrations]]
id = "0001-a"
author = "a"
changes = []

[[migrations]]
id = "0001-b"
author = "a"
changes = []
"#,
    );

    let err = MigrationLoader::load_from_dir(temp.path()).unwrap_err();
    assert!(err.to_string().contains("Duplicate migration prefix"));
}

#[test]
fn migration_file_deserializes_from_toml() {
    let raw = r#"
[[migrations]]
id = "0001-a"
author = "a"
since = "0.2.0"
changes = [{ sql = "SELECT 1" }]
"#;
    let file: MigrationFile = toml::from_str(raw).expect("parse");
    assert_eq!(file.migrations.len(), 1);
    assert_eq!(file.migrations[0].since.as_deref(), Some("0.2.0"));
}
