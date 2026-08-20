//! Unit tests for `runner.rs`.

use quench_db::catalog::ModuleScope;
use quench_db::plan::{MigrationPlan, ResolvedModule};
use quench_db::runner::*;
use std::collections::BTreeMap;

#[test]
fn rejects_injection_in_table_names() {
    assert!(validate_identifier("ledger table", "forge_migrations").is_ok());
    assert!(validate_identifier("ledger table", "forge migrations").is_err());
    assert!(validate_identifier("ledger table", "x; DROP TABLE y").is_err());
    assert!(validate_identifier("ledger table", "1st").is_err());
    assert!(validate_identifier("ledger table", "").is_err());
}

#[test]
fn advisory_lock_key_is_stable() {
    assert_eq!(
        advisory_lock_key("public.forge_migrations"),
        advisory_lock_key("public.forge_migrations")
    );
    assert_ne!(
        advisory_lock_key("public.forge_migrations"),
        advisory_lock_key("other.forge_migrations")
    );
}

fn resolved(module: &str, schema: &str, scope: ModuleScope) -> ResolvedModule {
    ResolvedModule {
        module: module.to_string(),
        version: semver::Version::new(0, 1, 0),
        schema: schema.to_string(),
        scope,
        required_by: None,
        variables: BTreeMap::new(),
    }
}

fn result(id: &str, outcome: MigrationOutcome) -> MigrationResult {
    MigrationResult {
        id: id.to_string(),
        module: "core".to_string(),
        schema: "core".to_string(),
        migration_id: "0001-a".to_string(),
        outcome,
    }
}

#[test]
fn apply_report_counts_by_outcome() {
    let report = ApplyReport {
        results: vec![
            result("a", MigrationOutcome::Applied),
            result("b", MigrationOutcome::Applied),
            result("c", MigrationOutcome::Skipped),
        ],
        dry_run: false,
    };
    assert_eq!(report.count(MigrationOutcome::Applied), 2);
    assert_eq!(report.count(MigrationOutcome::Skipped), 1);
    assert_eq!(report.count(MigrationOutcome::Pending), 0);
}

#[test]
fn apply_report_with_outcome_filters_to_matching_results() {
    let report = ApplyReport {
        results: vec![
            result("a", MigrationOutcome::Applied),
            result("b", MigrationOutcome::Pending),
        ],
        dry_run: true,
    };
    let pending: Vec<&str> = report
        .with_outcome(MigrationOutcome::Pending)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(pending, vec!["b"]);
}

#[test]
fn resettable_schemas_excludes_public_the_ledger_schema_and_database_scoped_modules() {
    let plan = MigrationPlan {
        modules: vec![
            resolved("sage", "sage", ModuleScope::Schema),
            resolved("core", "public", ModuleScope::Schema),
            resolved("pgvector", "public", ModuleScope::Database),
            resolved("quench-auth", "foundry", ModuleScope::Schema),
        ],
        migrations: vec![],
    };

    let runner = MigrationRunner::new().ledger_schema("foundry");
    let schemas = runner.resettable_schemas(&plan);
    assert_eq!(schemas, vec!["sage".to_string()]);
}

#[test]
fn resettable_schemas_is_sorted_and_deduplicated() {
    let plan = MigrationPlan {
        modules: vec![
            resolved("a", "zeta", ModuleScope::Schema),
            resolved("b", "alpha", ModuleScope::Schema),
            resolved("c", "alpha", ModuleScope::Schema),
        ],
        migrations: vec![],
    };
    let schemas = MigrationRunner::new().resettable_schemas(&plan);
    assert_eq!(schemas, vec!["alpha".to_string(), "zeta".to_string()]);
}

#[test]
fn builder_methods_chain_and_set_every_field() {
    let runner = MigrationRunner::new()
        .ledger_schema("custom_schema")
        .ledger_table("custom_ledger")
        .module_table("custom_modules")
        .allow_drift(true)
        .dry_run(true);

    let debug = format!("{runner:?}");
    assert!(debug.contains("custom_schema"));
    assert!(debug.contains("custom_ledger"));
    assert!(debug.contains("custom_modules"));
    assert!(debug.contains("allow_drift: true"));
    assert!(debug.contains("dry_run: true"));
}

#[test]
fn default_runner_uses_the_documented_defaults() {
    let debug = format!("{:?}", MigrationRunner::new());
    assert!(debug.contains(DEFAULT_LEDGER_SCHEMA));
    assert!(debug.contains(DEFAULT_LEDGER_TABLE));
    assert!(debug.contains(DEFAULT_MODULE_TABLE));
    assert!(debug.contains("allow_drift: false"));
    assert!(debug.contains("dry_run: false"));
}

#[test]
fn migration_outcome_equality_distinguishes_every_variant() {
    assert_eq!(MigrationOutcome::Applied, MigrationOutcome::Applied);
    assert_ne!(MigrationOutcome::Applied, MigrationOutcome::Skipped);
    assert_ne!(MigrationOutcome::Skipped, MigrationOutcome::Pending);
}

#[test]
fn reset_report_default_is_empty() {
    let report = ResetReport::default();
    assert!(report.schemas.is_empty());
    assert_eq!(report.forgotten_migrations, 0);
}
