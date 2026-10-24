//! Exercises the Postgres backend against a real server.
//!
//! Skipped unless `DB_TEST_POSTGRES_URL` is set, so the suite stays runnable
//! without infrastructure - mirrors `quench-cache/tests/redis_store.rs`.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_db::prelude::*;
use quench_db::{Crud, Database};
use quench_serde::{Deserialize, Serialize};

fn url() -> Option<String> {
    std::env::var("DB_TEST_POSTGRES_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Widget {
    id: String,
    name: String,
}

impl Model for Widget {
    fn table_name() -> String {
        "quench_db_test_widgets".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

async fn widgets_table(db: &PostgresDb) {
    db.execute("DROP TABLE IF EXISTS quench_db_test_widgets")
        .await
        .expect("drop table");
    db.execute("CREATE TABLE quench_db_test_widgets (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .expect("create table");
}

#[quench_test::test]
async fn connect_and_execute_a_statement() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    db.execute("SELECT 1").await.expect("execute");
}

#[quench_test::test]
async fn repository_crud_round_trips_through_postgres() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    widgets_table(&db).await;

    let repo = PostgresRepository::<Widget>::new(db);
    let widget = Widget {
        id: "1".to_string(),
        name: "sprocket".to_string(),
    };
    let created = repo.create(&widget).await.expect("create");
    assert_eq!(created, widget);

    let found = repo.read("1").await.expect("read").expect("present");
    assert_eq!(found, widget);

    let updated = Widget {
        id: "1".to_string(),
        name: "gizmo".to_string(),
    };
    repo.update(&updated).await.expect("update");
    assert_eq!(
        repo.read("1").await.expect("read").expect("present").name,
        "gizmo"
    );

    let matches = repo.find_by("name", "gizmo").await.expect("find_by");
    assert_eq!(matches.len(), 1);

    assert_eq!(repo.list().await.expect("list").len(), 1);

    repo.delete("1").await.expect("delete");
    assert!(repo.read("1").await.expect("read").is_none());
}

#[quench_test::test]
async fn db_connect_with_a_url_gives_the_postgres_backend() {
    let Some(url) = url() else { return };
    let db = Db::connect(&url).await.expect("connect");
    assert!(matches!(db, Db::Postgres(_)));
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Numbered {
    id: i64,
    label: String,
    tags: Vec<String>,
    meta: quench_serde::Value,
    seen_at: quench_core::time::Time,
    maybe: Option<String>,
}

impl Model for Numbered {
    fn table_name() -> String {
        "quench_db_test_numbered".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "label", "tags", "meta", "seen_at", "maybe"]
    }
}

#[quench_test::test]
async fn models_with_numbers_json_times_and_arrays_round_trip() {
    let Some(url) = url() else { return };
    let db = Db::connect(&url).await.expect("connect");
    db.execute("DROP TABLE IF EXISTS quench_db_test_numbered; CREATE TABLE quench_db_test_numbered (id BIGINT PRIMARY KEY, label TEXT NOT NULL, tags TEXT[] NOT NULL, meta JSONB NOT NULL, seen_at TIMESTAMPTZ NOT NULL, maybe TEXT)")
        .await
        .expect("script with several statements");
    let repo = db.repository::<Numbered>();
    let item = Numbered {
        id: 7,
        label: "seven".to_string(),
        tags: vec!["a b".to_string(), "c\"d".to_string()],
        meta: quench_serde::json!({"nested": [1, 2, {"k": null}]}),
        seen_at: quench_core::time::Time::from_unix_nanos(1_792_719_480, 123_456_000),
        maybe: None,
    };
    assert_eq!(repo.create(&item).await.expect("create"), item);
    let mut changed = item.clone();
    changed.label = "SEVEN".to_string();
    changed.maybe = Some("now set".to_string());
    assert_eq!(
        repo.update(&changed)
            .await
            .expect("update with a numeric key"),
        changed
    );
    assert_eq!(repo.read("7").await.expect("read"), Some(changed.clone()));
    assert_eq!(
        repo.find_by("label", "SEVEN").await.expect("find_by"),
        vec![changed]
    );
    assert!(
        repo.find_by("nope", "x").await.is_err(),
        "undeclared columns are refused"
    );
    assert!(repo.read("999").await.expect("read").is_none());
    repo.delete("7").await.expect("delete");
    assert!(repo.list().await.expect("list").is_empty());
    db.execute("DROP TABLE quench_db_test_numbered")
        .await
        .expect("drop");
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Gadget {
    id: String,
    name: String,
}

impl Model for Gadget {
    fn table_name() -> String {
        "quench_db_test_gadgets".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

#[quench_test::test]
async fn errors_surface_as_db_errors() {
    let Some(url) = url() else { return };
    let db = Db::connect(&url).await.expect("connect");
    assert!(matches!(
        db.execute("SELEC 1").await,
        Err(DbError::QueryError(_))
    ));
    db.execute("DROP TABLE IF EXISTS quench_db_test_gadgets; CREATE TABLE quench_db_test_gadgets (id TEXT PRIMARY KEY, name TEXT NOT NULL)").await.expect("create");
    let repo = db.repository::<Gadget>();
    let gadget = Gadget {
        id: "dup".to_string(),
        name: "x".to_string(),
    };
    repo.create(&gadget).await.expect("create");
    assert!(
        matches!(repo.create(&gadget).await, Err(DbError::QueryError(m)) if m.contains("duplicate"))
    );
    assert!(
        repo.update(&Gadget {
            id: "missing".to_string(),
            name: "y".to_string()
        })
        .await
        .is_err(),
        "updating a row that is not there"
    );
    assert!(matches!(
        PostgresDb::new("not a url").await,
        Err(DbError::ConnectionError(_))
    ));
    assert!(matches!(
        PostgresDb::new("postgres://127.0.0.1:1/x").await,
        Err(DbError::ConnectionError(_))
    ));
    let rendered = format!("{db:?} {repo:?}");
    assert!(rendered.contains("PostgresDb") && rendered.contains("Postgres"));
    db.execute("DROP TABLE quench_db_test_gadgets")
        .await
        .expect("drop");
}

#[quench_test::test]
async fn migrate_applies_each_migration_once() {
    let Some(url) = url() else { return };
    let table = format!(
        "quench_db_test_ledger_{}",
        quench_core::uuid::Uuid::new_v4().simple()
    );
    quench_core::env::set("DB_MIGRATION_TABLE", &table);
    quench_core::env::set("DB_POOL_MAX_SIZE", "2");
    let db = Db::connect(&url).await.expect("connect");
    let migration = |id: &str, sql: &str| Migration {
        id: id.to_string(),
        author: "tests".to_string(),
        since: None,
        changes: vec![ChangeSet::Sql(sql.to_string())],
    };
    let batch = || {
        vec![
            migration(
                "0001-create",
                "CREATE TABLE quench_db_test_migrated (n INT); INSERT INTO quench_db_test_migrated VALUES (1)",
            ),
            migration(
                "0002-more",
                "INSERT INTO quench_db_test_migrated VALUES (2)",
            ),
        ]
    };
    db.execute("DROP TABLE IF EXISTS quench_db_test_migrated")
        .await
        .expect("drop");
    db.migrate(batch()).await.expect("first run");
    db.migrate(batch())
        .await
        .expect("a second run changes nothing");
    let Db::Postgres(pg) = &db else {
        unreachable!("a URL was given")
    };
    let rows = pg
        .pool()
        .query("SELECT count(*) FROM quench_db_test_migrated", &[])
        .await
        .expect("count");
    assert_eq!(
        rows[0].get::<i64, _>(&0usize).expect("count"),
        2,
        "each migration ran exactly once"
    );
    let applied = pg
        .pool()
        .query(&format!("SELECT id FROM {table} ORDER BY id"), &[])
        .await
        .expect("ledger");
    assert_eq!(applied.len(), 2);

    let broken = db
        .migrate(vec![migration(
            "0003-broken",
            "INSERT INTO quench_db_test_migrated VALUES ('x')",
        )])
        .await;
    assert!(broken.is_err());
    let after = pg
        .pool()
        .query(&format!("SELECT id FROM {table}"), &[])
        .await
        .expect("ledger");
    assert_eq!(
        after.len(),
        2,
        "a failed migration is rolled back and not recorded"
    );
    db.execute(&format!(
        "DROP TABLE quench_db_test_migrated; DROP TABLE {table}"
    ))
    .await
    .expect("cleanup");
    quench_core::env::remove("DB_MIGRATION_TABLE");
    quench_core::env::remove("DB_POOL_MAX_SIZE");
}

quench_test::main!();
