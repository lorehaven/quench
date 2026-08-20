//! Unit tests for `backends/in_memory.rs`.

use quench_db::{Crud, Database, InMemoryDb, InMemoryRepository, Model};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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

fn repo() -> InMemoryRepository<Widget> {
    InMemoryRepository::new(InMemoryDb::new())
}

#[tokio::test]
async fn create_then_read_round_trips_the_model() {
    let repo = repo();
    let widget = Widget {
        id: "1".to_string(),
        name: "sprocket".to_string(),
    };
    let created = repo.create(&widget).await.expect("create");
    assert_eq!(created, widget);

    let found = repo.read("1").await.expect("read").expect("present");
    assert_eq!(found, widget);
}

#[tokio::test]
async fn read_of_a_missing_id_is_none_not_an_error() {
    let repo = repo();
    assert!(repo.read("missing").await.expect("read").is_none());
}

#[tokio::test]
async fn update_overwrites_the_existing_row() {
    let repo = repo();
    let widget = Widget {
        id: "1".to_string(),
        name: "sprocket".to_string(),
    };
    repo.create(&widget).await.expect("create");

    let updated = Widget {
        id: "1".to_string(),
        name: "gizmo".to_string(),
    };
    repo.update(&updated).await.expect("update");

    let found = repo.read("1").await.expect("read").expect("present");
    assert_eq!(found.name, "gizmo");
}

#[tokio::test]
async fn delete_removes_the_row() {
    let repo = repo();
    let widget = Widget {
        id: "1".to_string(),
        name: "sprocket".to_string(),
    };
    repo.create(&widget).await.expect("create");
    repo.delete("1").await.expect("delete");
    assert!(repo.read("1").await.expect("read").is_none());
}

#[tokio::test]
async fn delete_of_a_missing_id_is_not_an_error() {
    let repo = repo();
    assert!(repo.delete("missing").await.is_ok());
}

#[tokio::test]
async fn list_returns_every_row() {
    let repo = repo();
    repo.create(&Widget {
        id: "1".to_string(),
        name: "a".to_string(),
    })
    .await
    .expect("create");
    repo.create(&Widget {
        id: "2".to_string(),
        name: "b".to_string(),
    })
    .await
    .expect("create");

    let mut all = repo.list().await.expect("list");
    all.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "a");
    assert_eq!(all[1].name, "b");
}

#[tokio::test]
async fn find_by_matches_only_the_string_valued_column() {
    let repo = repo();
    repo.create(&Widget {
        id: "1".to_string(),
        name: "sprocket".to_string(),
    })
    .await
    .expect("create");
    repo.create(&Widget {
        id: "2".to_string(),
        name: "gizmo".to_string(),
    })
    .await
    .expect("create");

    let found = repo.find_by("name", "gizmo").await.expect("find_by");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "2");

    let none = repo
        .find_by("name", "nothing-matches")
        .await
        .expect("find_by");
    assert!(none.is_empty());
}

#[tokio::test]
async fn find_by_rejects_a_column_the_model_does_not_declare() {
    let repo = repo();
    assert!(repo.find_by("not_a_column", "x").await.is_err());
}

#[tokio::test]
async fn create_rejects_a_model_missing_its_primary_key() {
    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct NoId {
        name: String,
    }
    impl Model for NoId {
        fn table_name() -> String {
            "no_id".to_string()
        }
        fn columns() -> Vec<&'static str> {
            vec!["name"]
        }
    }

    let repo = InMemoryRepository::new(InMemoryDb::new());
    let result = repo
        .create(&NoId {
            name: "x".to_string(),
        })
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn in_memory_db_execute_and_migrate_are_no_ops() {
    let db = InMemoryDb::new();
    assert!(db.execute("CREATE TABLE anything").await.is_ok());
    assert!(db.migrate(vec![]).await.is_ok());
}

#[test]
fn in_memory_db_debug_names_the_type() {
    assert_eq!(format!("{:?}", InMemoryDb::new()), "InMemoryDb");
}
