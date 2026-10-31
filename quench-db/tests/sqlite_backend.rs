//! The SQLite backend: a database file in a temporary directory, so it runs everywhere.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_db::prelude::*;
use quench_db::{ChangeSet, Crud, Database, Migration, SqliteDb, SqliteRepository};
use quench_serde::{Deserialize, Serialize};

fn temp_url() -> (quench_core::tmp::TempDir, String) {
    let dir = quench_core::tmp::TempDir::new().expect("temp dir");
    let url = format!("sqlite://{}", dir.path().join("test.db").display());
    (dir, url)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Widget {
    id: String,
    name: String,
}

impl Model for Widget {
    fn table_name() -> String {
        "widgets".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

#[quench_test::test]
async fn repository_crud_round_trips_through_sqlite() {
    let (_dir, url) = temp_url();
    let db = SqliteDb::open(&url).expect("open");
    db.execute("CREATE TABLE widgets (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .expect("create table");
    let repo = SqliteRepository::<Widget>::new(db);
    let widget = Widget {
        id: "1".into(),
        name: "sprocket".into(),
    };
    assert_eq!(repo.create(&widget).await.expect("create"), widget);
    assert_eq!(repo.read("1").await.expect("read"), Some(widget.clone()));
    assert_eq!(repo.read("2").await.expect("read"), None);
    let updated = Widget {
        id: "1".into(),
        name: "gizmo".into(),
    };
    assert_eq!(repo.update(&updated).await.expect("update").name, "gizmo");
    assert_eq!(
        repo.find_by("name", "gizmo").await.expect("find_by").len(),
        1
    );
    assert!(
        repo.find_by("nope", "x").await.is_err(),
        "an unknown column is refused, not interpolated"
    );
    assert!(
        repo.find_by("name", "x'; DROP TABLE widgets; --")
            .await
            .expect("find_by")
            .is_empty()
    );
    assert_eq!(repo.list().await.expect("list").len(), 1);
    assert!(matches!(
        repo.update(&Widget {
            id: "9".into(),
            name: "ghost".into()
        })
        .await,
        Err(DbError::NotFound)
    ));
    assert!(
        repo.create(&widget).await.is_err(),
        "a second row with the same key is refused"
    );
    repo.delete("1").await.expect("delete");
    assert!(repo.read("1").await.expect("read").is_none());
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Numbered {
    id: i64,
    label: String,
    active: bool,
    ratio: f64,
    tags: Vec<String>,
    meta: quench_serde::Value,
    maybe: Option<String>,
}

impl Model for Numbered {
    fn table_name() -> String {
        "numbered".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "label", "active", "ratio", "tags", "meta", "maybe"]
    }
}

#[quench_test::test]
async fn models_with_numbers_booleans_json_and_options_round_trip() {
    let (_dir, url) = temp_url();
    let db = Db::connect(&url).await.expect("connect");
    assert!(matches!(db, Db::Sqlite(_)));
    db.execute(
        "CREATE TABLE numbered (id INTEGER PRIMARY KEY, label TEXT NOT NULL, active BOOLEAN NOT NULL, \
         ratio REAL NOT NULL, tags JSON NOT NULL, meta JSON NOT NULL, maybe TEXT); \
         CREATE INDEX numbered_label ON numbered (label)",
    )
    .await
    .expect("script with several statements");
    let repo = db.repository::<Numbered>();
    let item = Numbered {
        id: 7,
        label: "seven".into(),
        active: true,
        ratio: 0.5,
        tags: vec!["a".into(), "b c".into()],
        meta: quench_serde::json::from_str(r#"{"k": [1, 2, {"z": null}], "s": "t"}"#)
            .expect("json"),
        maybe: None,
    };
    assert_eq!(repo.create(&item).await.expect("create"), item);
    assert_eq!(
        repo.read("7").await.expect("read"),
        Some(item.clone()),
        "an integer key read by its text"
    );
    let changed = Numbered {
        active: false,
        maybe: Some("now".into()),
        tags: vec![],
        ..item
    };
    assert_eq!(repo.update(&changed).await.expect("update"), changed);
    assert_eq!(
        repo.find_by("label", "seven").await.expect("find_by"),
        vec![changed.clone()]
    );
    assert_eq!(repo.find_by("id", "7").await.expect("find_by").len(), 1);
}

fn migration(id: &str, sql: &str) -> Migration {
    Migration {
        id: id.into(),
        author: "tests".into(),
        since: None,
        changes: vec![ChangeSet::Sql(sql.into())],
    }
}

#[quench_test::test]
async fn migrations_apply_once_and_a_failing_one_leaves_nothing() {
    let (_dir, url) = temp_url();
    let db = SqliteDb::open(&url).expect("open");
    let first = migration(
        "001",
        "CREATE TABLE a (id INTEGER PRIMARY KEY, v TEXT); INSERT INTO a (v) VALUES ('x')",
    );
    db.migrate(vec![first.clone()]).await.expect("first");
    db.migrate(vec![
        first.clone(),
        migration("002", "CREATE TABLE b (id INTEGER PRIMARY KEY)"),
    ])
    .await
    .expect("second");
    // The same migration again is not run again (it would fail: the table exists).
    db.migrate(vec![first]).await.expect("idempotent");
    let bad = migration(
        "003",
        "CREATE TABLE c (id INTEGER PRIMARY KEY); INSERT INTO a (nope) VALUES (1)",
    );
    assert!(db.migrate(vec![bad.clone()]).await.is_err());
    // Nothing of the failed migration is left: not the table, not the ledger row.
    db.execute("CREATE TABLE c (ok INTEGER)")
        .await
        .expect("c was rolled back");
    assert!(
        db.migrate(vec![bad]).await.is_err(),
        "and it is tried again, not remembered as applied"
    );
    let repo = SqliteRepository::<Widget>::new(db.clone());
    db.execute("CREATE TABLE widgets (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .expect("widgets");
    assert!(repo.list().await.expect("list").is_empty());
}

quench_test::main!();
