//! The ways loading a catalog and building a plan can fail, and the messages they give.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::tmp::TempDir;
use quench_db::prelude::{Catalog, InstallRequest, MigrationPlan, PlanError};
use quench_db::{CatalogError, ModuleManifest};
use std::path::Path;

fn module(root: &Path, name: &str, manifest: &str, migrations: &[(&str, &str)]) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("create module dir");
    std::fs::write(dir.join("module.toml"), manifest).expect("write manifest");
    for (file, body) in migrations {
        std::fs::write(dir.join(file), body).expect("write migration");
    }
}

const ONE_MIGRATION: &str =
    "[[migrations]]\nid = \"0001-a\"\nauthor = \"t\"\nchanges = [{ sql = \"SELECT 1\" }]\n";

#[quench_test::test]
fn requirement_shapes_parse_or_explain_themselves() {
    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "dep",
        "name = \"dep\"\nversion = \"1.2.0\"\n",
        &[],
    );
    module(
        temp.path(),
        "main",
        "name = \"main\"\nversion = \"0.1.0\"\nrequires = [\"dep\", \"dep@^1\", { module = \"dep\" }, { module = \"dep\", version = \"^1.1\", schema = \"s\" }]\n",
        &[],
    );
    let catalog = Catalog::load(temp.path()).expect("all four forms are valid");
    let main: &ModuleManifest = &catalog.get("main").unwrap().manifest;
    assert_eq!(main.requires.len(), 4);
    assert!(main.requires[0].version.to_string().contains('*'));
    assert_eq!(main.requires[3].schema.as_deref(), Some("s"));

    for (spec, needle) in [
        ("requires = [\"@^1\"]", "missing module name"),
        ("requires = [\"dep@not-a-req\"]", ""),
    ] {
        let temp = TempDir::new().unwrap();
        module(
            temp.path(),
            "main",
            &format!("name = \"main\"\nversion = \"0.1.0\"\n{spec}\n"),
            &[],
        );
        let err = Catalog::load(temp.path()).unwrap_err();
        assert!(matches!(err, CatalogError::BadManifest { .. }), "{err}");
        assert!(err.to_string().contains(needle), "{err}");
    }
    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "main",
        "name = \"main\"\nversion = \"0.1.0\"\nrequires = [5]\n",
        &[],
    );
    assert!(
        Catalog::load(temp.path())
            .unwrap_err()
            .to_string()
            .contains("requirement string or table")
    );
    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "main",
        "name = \"main\"\nversion = \"0.1.0\"\nrequires = [{ module = \"x\", version = \"???\" }]\n",
        &[],
    );
    assert!(Catalog::load(temp.path()).is_err());
}

#[quench_test::test]
fn broken_catalogs_are_rejected_with_the_cause() {
    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("a-file"), "x").unwrap();
    let err = Catalog::load(temp.path().join("a-file")).unwrap_err();
    assert!(
        matches!(
            err,
            CatalogError::Io { .. } | CatalogError::MissingCatalog { .. }
        ),
        "{err}"
    );
    assert!(err.to_string().contains("a-file"));

    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "bad",
        "name = \"bad\"\nversion = not-toml\n",
        &[],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::BadManifest { .. }), "{err}");

    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "bad",
        "name = \"bad\"\nversion = \"0.1.0\"\n",
        &[("0001-x.toml", "this is = not [valid")],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::BadMigrations { .. }), "{err}");

    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "one",
        "name = \"same\"\nversion = \"0.1.0\"\n",
        &[],
    );
    module(
        temp.path(),
        "two",
        "name = \"same\"\nversion = \"0.1.0\"\n",
        &[],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(matches!(err, CatalogError::DuplicateModule { .. }), "{err}");

    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "dup",
        "name = \"dup\"\nversion = \"0.1.0\"\n",
        &[
            ("0001-a.toml", ONE_MIGRATION),
            ("0002-b.toml", ONE_MIGRATION),
        ],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(
        matches!(
            err,
            CatalogError::DuplicateMigrationId { .. } | CatalogError::BadMigrations { .. }
        ),
        "{err}"
    );

    let temp = TempDir::new().unwrap();
    let tagged =
        "[[migrations]]\nid = \"0001-a\"\nauthor = \"t\"\nsince = \"banana\"\nchanges = []\n";
    module(
        temp.path(),
        "tag",
        "name = \"tag\"\nversion = \"0.1.0\"\n",
        &[("0001-a.toml", tagged)],
    );
    let err = Catalog::load(temp.path()).unwrap_err();
    assert!(
        matches!(err, CatalogError::BadMigrationVersion { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("0001-a"));
}

#[quench_test::test]
fn directories_without_a_manifest_are_ignored() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join("not-a-module")).unwrap();
    std::fs::write(temp.path().join("stray-file.txt"), "x").unwrap();
    module(
        temp.path(),
        "real",
        "name = \"real\"\nversion = \"0.1.0\"\n",
        &[],
    );
    let catalog = Catalog::load(temp.path()).unwrap();
    assert_eq!(catalog.len(), 1);
}

#[quench_test::test]
fn plan_errors_describe_what_went_wrong() {
    let temp = TempDir::new().unwrap();
    module(
        temp.path(),
        "core",
        "name = \"core\"\nversion = \"0.2.0\"\n",
        &[("0001-a.toml", ONE_MIGRATION)],
    );
    let catalog = Catalog::load(temp.path()).unwrap();

    let nothing = MigrationPlan::resolve(&catalog, &[]).unwrap_err();
    assert!(matches!(nothing, PlanError::NothingToInstall));
    assert_eq!(nothing.to_string(), "no modules requested");

    let twice = MigrationPlan::resolve(
        &catalog,
        &[
            InstallRequest::parse("core@0.1.0:s").unwrap(),
            InstallRequest::parse("core@0.2.0:s").unwrap(),
        ],
    )
    .unwrap_err();
    assert!(
        matches!(twice, PlanError::ConflictingVersions { .. }),
        "{twice}"
    );
    assert!(twice.to_string().contains("requested twice"));

    let future = MigrationPlan::resolve(&catalog, &[InstallRequest::parse("core@9.0.0").unwrap()])
        .unwrap_err();
    assert!(
        matches!(future, PlanError::VersionNotInCatalog { .. }),
        "{future}"
    );

    for bad in ["@1.0.0", ":schema", "core@", ""] {
        let err = InstallRequest::parse(bad).unwrap_err();
        assert!(
            matches!(err, PlanError::BadInstallSpec { .. }),
            "{bad:?}: {err}"
        );
        assert!(err.to_string().contains("invalid install spec"));
    }
    let same_twice = MigrationPlan::resolve(
        &catalog,
        &[
            InstallRequest::parse("core:s").unwrap(),
            InstallRequest::parse("core:s").unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        same_twice.migrations.len(),
        1,
        "asking for the same instance twice plans it once"
    );
    assert!(!same_twice.is_empty());
    assert!(InstallRequest::new("core").variables.is_empty());
}

quench_test::main!();
