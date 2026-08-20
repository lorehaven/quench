//! Exercises the Postgres backend against a real server.
//!
//! Skipped unless `DB_TEST_POSTGRES_URL` is set, so the suite stays runnable
//! without infrastructure - mirrors `quench-cache/tests/redis_store.rs`.

use quench_db::prelude::*;
use quench_db::{Crud, Database};
use serde::{Deserialize, Serialize};

fn url() -> Option<String> {
    std::env::var("DB_TEST_POSTGRES_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
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

#[tokio::test]
async fn connect_and_execute_a_statement() {
    let Some(url) = url() else { return };
    let db = PostgresDb::new(&url).await.expect("connect");
    db.execute("SELECT 1").await.expect("execute");
}

#[tokio::test]
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

#[tokio::test]
async fn db_connect_with_a_url_gives_the_postgres_backend() {
    let Some(url) = url() else { return };
    let db = Db::connect(&url).await.expect("connect");
    assert!(matches!(db, Db::Postgres(_)));
}
