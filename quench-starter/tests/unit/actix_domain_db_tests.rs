//! Unit tests for `actix/domain/db.rs`.

use quench_db::prelude::Db;
use quench_starter::actix::domain::db::DbWrapper;

#[actix_web::test]
async fn an_empty_url_without_the_opt_in_refuses_to_start_but_succeeds_once_allowed() {
    // Sequential within one test so the two states of `ALLOW_IN_MEMORY_DB`
    // never race against another test in this binary.
    unsafe {
        std::env::remove_var("ALLOW_IN_MEMORY_DB");
    }

    let panicked = tokio::spawn(DbWrapper::init(String::new())).await;
    match panicked {
        Err(join_error) => assert!(
            join_error.is_panic(),
            "an empty URL with no opt-in must refuse to start via panic, not a normal error"
        ),
        Ok(_) => panic!("an empty URL with no opt-in must refuse to start"),
    }

    unsafe {
        std::env::set_var("ALLOW_IN_MEMORY_DB", "true");
    }
    let wrapper = DbWrapper::init(String::new()).await;
    assert!(matches!(wrapper.db, Db::InMemory(_)));

    // Same opt-in also covers `init_env`, reading an (absent) DATABASE_URL/
    // POSTGRES_URL from the environment - kept in this same test so the two
    // never race over `ALLOW_IN_MEMORY_DB` with a test in another thread.
    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("POSTGRES_URL");
    }
    let wrapper = DbWrapper::init_env().await;
    assert!(matches!(wrapper.db, Db::InMemory(_)));

    unsafe {
        std::env::remove_var("ALLOW_IN_MEMORY_DB");
    }
}
