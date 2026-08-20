//! Unit tests for `lib.rs`.

use quench_db::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
struct TestModel {
    id: String,
    name: String,
}

impl Model for TestModel {
    fn table_name() -> String {
        "test_table".to_string()
    }

    fn columns() -> Vec<&'static str> {
        vec!["id", "name"]
    }
}

#[test]
fn test_model_trait() {
    assert_eq!(TestModel::table_name(), "test_table");
}

#[test]
fn model_primary_key_name_defaults_to_id() {
    assert_eq!(TestModel::primary_key_name(), "id");
}

#[tokio::test]
async fn connect_with_an_empty_url_gives_an_in_memory_db() {
    let db = Db::connect("").await.expect("connect");
    assert!(matches!(db, Db::InMemory(_)));
}

#[tokio::test]
async fn db_execute_and_migrate_dispatch_to_the_in_memory_backend() {
    let db = Db::connect("").await.expect("connect");
    assert!(db.execute("anything").await.is_ok());
    assert!(db.migrate(vec![]).await.is_ok());
}

#[tokio::test]
async fn repository_dispatches_crud_to_the_in_memory_backend() {
    let db = Db::connect("").await.expect("connect");
    let repo = db.repository::<TestModel>();
    let model = TestModel {
        id: "1".to_string(),
        name: "x".to_string(),
    };
    repo.create(&model).await.expect("create");
    let found = repo.read("1").await.expect("read").expect("present");
    assert_eq!(found.name, "x");

    let updated = TestModel {
        id: "1".to_string(),
        name: "y".to_string(),
    };
    repo.update(&updated).await.expect("update");
    assert_eq!(
        repo.read("1").await.expect("read").expect("present").name,
        "y"
    );

    let matches = repo.find_by("name", "y").await.expect("find_by");
    assert_eq!(matches.len(), 1);

    assert_eq!(repo.list().await.expect("list").len(), 1);

    repo.delete("1").await.expect("delete");
    assert!(repo.read("1").await.expect("read").is_none());
}
