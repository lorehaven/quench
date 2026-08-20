//! Unit tests for `catalog.rs`.

use quench_db::catalog::{Catalog, CatalogError, ModuleScope, migration_since};
use quench_db::migrations::Migration;
use std::path::Path;

fn module(root: &Path, name: &str, manifest: &str, migrations: &[(&str, &str)]) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("create module dir");
    std::fs::write(dir.join("module.toml"), manifest).expect("write manifest");
    for (file, body) in migrations {
        std::fs::write(dir.join(file), body).expect("write migration");
    }
}

#[test]
fn module_scope_defaults_to_schema() {
    assert_eq!(ModuleScope::default(), ModuleScope::Schema);
}

#[test]
fn migration_since_defaults_to_zero_when_untagged() {
    let migration = Migration {
        id: "0001-a".to_string(),
        author: "a".to_string(),
        since: None,
        changes: vec![],
    };
    assert_eq!(migration_since(&migration), semver::Version::new(0, 0, 0));
}

#[test]
fn migration_since_parses_a_tagged_version() {
    let migration = Migration {
        id: "0001-a".to_string(),
        author: "a".to_string(),
        since: Some("1.2.3".to_string()),
        changes: vec![],
    };
    assert_eq!(migration_since(&migration), semver::Version::new(1, 2, 3));
}

#[test]
fn load_rejects_a_missing_root() {
    let err = Catalog::load("/nonexistent/path/does-not-exist").unwrap_err();
    assert!(matches!(err, CatalogError::MissingCatalog { .. }));
}

#[test]
fn load_finds_modules_and_exposes_them() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.1.0\"\n",
        &[(
            "0001-schema.toml",
            r#"
[[migrations]]
id = "0001-a"
author = "a"
changes = [{ sql = "SELECT 1" }]
"#,
        )],
    );

    let catalog = Catalog::load(temp.path()).expect("load");
    assert_eq!(catalog.len(), 1);
    assert!(!catalog.is_empty());
    let core = catalog.get("core").expect("core module");
    assert_eq!(core.name(), "core");
    assert_eq!(core.version(), &semver::Version::new(0, 1, 0));
    assert_eq!(catalog.modules().count(), 1);
}

#[test]
fn fallback_schema_uses_the_module_name_for_schema_scoped_modules() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "sage",
        "name = \"sage\"\nversion = \"0.1.0\"\n",
        &[],
    );
    let catalog = Catalog::load(temp.path()).expect("load");
    let module = catalog.get("sage").expect("module");
    assert_eq!(module.manifest.fallback_schema(), "sage");
}

#[test]
fn fallback_schema_is_public_for_database_scoped_modules() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "pgvector",
        "name = \"pgvector\"\nversion = \"0.1.0\"\nscope = \"database\"\n",
        &[],
    );
    let catalog = Catalog::load(temp.path()).expect("load");
    let module = catalog.get("pgvector").expect("module");
    assert_eq!(module.manifest.fallback_schema(), "public");
}

#[test]
fn migrations_up_to_excludes_migrations_tagged_after_the_requested_version() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.3.0\"\n",
        &[(
            "0001-schema.toml",
            r#"
[[migrations]]
id = "0001-a"
author = "a"
since = "0.1.0"
changes = []

[[migrations]]
id = "0002-b"
author = "a"
since = "0.3.0"
changes = []
"#,
        )],
    );
    let catalog = Catalog::load(temp.path()).expect("load");
    let module = catalog.get("core").expect("module");

    let up_to_0_2 = module.migrations_up_to(&semver::Version::new(0, 2, 0));
    assert_eq!(up_to_0_2.len(), 1);
    assert_eq!(up_to_0_2[0].id, "0001-a");

    let up_to_0_3 = module.migrations_up_to(&semver::Version::new(0, 3, 0));
    assert_eq!(up_to_0_3.len(), 2);
}

#[test]
fn load_rejects_a_migration_since_ahead_of_its_module_version() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.1.0\"\n",
        &[(
            "0001-schema.toml",
            r#"
[[migrations]]
id = "0001-a"
author = "a"
since = "9.0.0"
changes = []
"#,
        )],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::MigrationAheadOfModule { .. }));
}

#[test]
fn load_rejects_an_unsatisfied_requirement() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.1.0\"\nrequires = [\"missing-dep@^1\"]\n",
        &[],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::UnknownModule { .. }));
}

#[test]
fn load_rejects_a_version_mismatched_requirement() {
    let temp = tempfile::TempDir::new().expect("tempdir");
    module(
        temp.path(),
        "dep",
        "name = \"dep\"\nversion = \"0.1.0\"\n",
        &[],
    );
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.1.0\"\nrequires = [\"dep@^2\"]\n",
        &[],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::UnsatisfiedRequirement { .. }));
}

#[test]
fn requirement_table_form_deserializes_with_an_explicit_schema() {
    use quench_db::catalog::ModuleManifest;
    let manifest: ModuleManifest = toml::from_str(
        "name = \"core\"\nversion = \"0.1.0\"\nrequires = [{ module = \"dep\", version = \"^1\", schema = \"shared\" }]\n",
    )
    .expect("parse");
    assert_eq!(manifest.requires.len(), 1);
    assert_eq!(manifest.requires[0].module, "dep");
    assert_eq!(manifest.requires[0].schema.as_deref(), Some("shared"));
}

#[test]
fn catalog_error_messages_are_readable() {
    let err = CatalogError::UnknownModule {
        name: "sage".to_string(),
        required_by: Some("core".to_string()),
    };
    assert_eq!(err.to_string(), "unknown module 'sage' required by 'core'");

    let err = CatalogError::UnknownModule {
        name: "sage".to_string(),
        required_by: None,
    };
    assert_eq!(err.to_string(), "unknown module 'sage'");
}
