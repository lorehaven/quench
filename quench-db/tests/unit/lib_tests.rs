//! Unit tests for `lib.rs`.

use quench_db::*;
use quench_serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[quench_test::test]
fn test_model_trait() {
    assert_eq!(TestModel::table_name(), "test_table");
}

#[quench_test::test]
fn model_primary_key_name_defaults_to_id() {
    assert_eq!(TestModel::primary_key_name(), "id");
}

#[quench_test::test]
async fn connect_with_an_empty_url_gives_an_in_memory_db() {
    let db = Db::connect("").await.expect("connect");
    assert!(matches!(db, Db::InMemory(_)));
}

#[quench_test::test]
async fn db_execute_and_migrate_dispatch_to_the_in_memory_backend() {
    let db = Db::connect("").await.expect("connect");
    assert!(db.execute("anything").await.is_ok());
    assert!(db.migrate(vec![]).await.is_ok());
}

#[quench_test::test]
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

#[cfg(not(feature = "postgres"))]
#[quench_test::test]
async fn a_url_needs_the_postgres_feature() {
    let err = quench_db::Db::connect("postgres://localhost/app")
        .await
        .unwrap_err();
    assert!(
        matches!(&err, quench_db::DbError::ConnectionError(m) if m.contains("`postgres` feature")),
        "{err}"
    );
    assert!(
        quench_db::Db::connect("").await.is_ok(),
        "the in-memory backend needs nothing"
    );
}
