//! Unit tests for `common/db.rs`.

use quench_db::prelude::Db;
use quench_starter::common::db::DbWrapper;

#[quench_test::test]
async fn an_empty_url_without_the_opt_in_refuses_to_start_but_succeeds_once_allowed() {
    // Sequential within one test so the two states of `ALLOW_IN_MEMORY_DB`
    // never race against another test in this binary.
    quench_core::env::remove("ALLOW_IN_MEMORY_DB");

    let panicked = tokio::spawn(DbWrapper::init(String::new())).await;
    match panicked {
        Err(join_error) => assert!(
            join_error.is_panic(),
            "an empty URL with no opt-in must refuse to start via panic, not a normal error"
        ),
        Ok(_) => panic!("an empty URL with no opt-in must refuse to start"),
    }

    quench_core::env::set("ALLOW_IN_MEMORY_DB", "true");
    let wrapper = DbWrapper::init(String::new()).await;
    assert!(matches!(wrapper.db, Db::InMemory(_)));

    // Same opt-in also covers `init_env`, reading an (absent) DATABASE_URL/
    // POSTGRES_URL from the environment - kept in this same test so the two
    // never race over `ALLOW_IN_MEMORY_DB` with a test in another thread.
    quench_core::env::remove("DATABASE_URL");
    quench_core::env::remove("POSTGRES_URL");
    let wrapper = DbWrapper::init_env().await;
    assert!(matches!(wrapper.db, Db::InMemory(_)));

    quench_core::env::remove("ALLOW_IN_MEMORY_DB");
}
