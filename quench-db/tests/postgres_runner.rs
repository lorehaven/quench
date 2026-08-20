//! Exercises `MigrationRunner` against a real Postgres server.
//!
//! Skipped unless `DB_TEST_POSTGRES_URL` is set - mirrors
//! `tests/postgres_backend.rs` and `quench-cache/tests/redis_store.rs`.

use quench_db::catalog::ModuleScope;
use quench_db::plan::{MigrationPlan, PlannedMigration, ResolvedModule};
use quench_db::prelude::*;
use std::collections::BTreeMap;

fn url() -> Option<String> {
    std::env::var("DB_TEST_POSTGRES_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

fn plan_for(schema: &str, table: &str) -> MigrationPlan {
    MigrationPlan {
        modules: vec![ResolvedModule {
            module: "runner-test".to_string(),
            version: semver::Version::new(0, 1, 0),
            schema: schema.to_string(),
            scope: ModuleScope::Schema,
            required_by: None,
            variables: BTreeMap::new(),
        }],
        migrations: vec![PlannedMigration {
            id: format!("runner-test@{schema}:0001-create"),
            module: "runner-test".to_string(),
            schema: schema.to_string(),
            migration_id: "0001-create".to_string(),
            author: "test".to_string(),
            since: semver::Version::new(0, 0, 0),
            module_version: semver::Version::new(0, 1, 0),
            statements: vec![format!(
                "CREATE SCHEMA IF NOT EXISTS {schema}; CREATE TABLE IF NOT EXISTS {schema}.{table} (id INT)"
            )],
            checksum: "test-checksum".to_string(),
        }],
    }
}

#[tokio::test]
async fn dry_run_reports_pending_without_writing_anything() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    let runner = MigrationRunner::new()
        .ledger_schema("quench_db_test_ledger")
        .dry_run(true);
    let plan = plan_for("quench_db_test_dry_run", "widgets");

    let report = runner.apply(&db, &plan).await.expect("apply");
    assert!(report.dry_run);
    assert_eq!(report.count(MigrationOutcome::Pending), 1);
}

#[tokio::test]
async fn apply_is_idempotent_and_reset_forgets_it() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    let runner = MigrationRunner::new().ledger_schema("quench_db_test_ledger2");
    let plan = plan_for("quench_db_test_apply", "widgets");

    let first = runner.apply(&db, &plan).await.expect("first apply");
    assert_eq!(first.count(MigrationOutcome::Applied), 1);

    let second = runner.apply(&db, &plan).await.expect("second apply");
    assert_eq!(second.count(MigrationOutcome::Skipped), 1);

    let installed = runner.installed_modules(&db).await.expect("installed");
    assert!(
        installed
            .iter()
            .any(|(module, schema, _)| module == "runner-test" && schema == "quench_db_test_apply")
    );

    let resettable = runner.resettable_schemas(&plan);
    assert_eq!(resettable, vec!["quench_db_test_apply".to_string()]);

    let report = runner.reset(&db, &plan).await.expect("reset");
    assert_eq!(report.schemas, vec!["quench_db_test_apply".to_string()]);
    assert_eq!(report.forgotten_migrations, 1);
}

#[tokio::test]
async fn drift_is_rejected_unless_explicitly_allowed() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    let runner = MigrationRunner::new().ledger_schema("quench_db_test_ledger3");
    let plan = plan_for("quench_db_test_drift", "widgets");
    runner.apply(&db, &plan).await.expect("first apply");

    let mut drifted = plan.clone();
    drifted.migrations[0].checksum = "changed-checksum".to_string();

    let rejected = runner.apply(&db, &drifted).await;
    assert!(rejected.is_err());

    let lenient = MigrationRunner::new()
        .ledger_schema("quench_db_test_ledger3")
        .allow_drift(true);
    let accepted = lenient
        .apply(&db, &drifted)
        .await
        .expect("apply with drift allowed");
    assert_eq!(accepted.count(MigrationOutcome::Skipped), 1);

    lenient.reset(&db, &plan).await.expect("cleanup");
}
