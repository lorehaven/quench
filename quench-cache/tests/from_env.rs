//! `CacheStore::from_env` reads process-global variables, so every case lives in one test.

#![cfg(feature = "redis")]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_cache::CacheStore;

#[quench_test::test]
async fn the_environment_picks_the_backend() {
    for key in ["CACHE_URL", "REDIS_URL", "CACHE_CLUSTER", "REDIS_CLUSTER"] {
        quench_core::env::remove(key);
    }
    let store = CacheStore::from_env("svc")
        .await
        .expect("in-memory by default");
    assert!(!store.is_shared());

    quench_core::env::set("CACHE_URL", "   ");
    assert!(
        !CacheStore::from_env("svc").await.unwrap().is_shared(),
        "a blank URL counts as unset"
    );

    // Forced cluster mode against a server that is not one fails when the topology is read.
    quench_core::env::set("REDIS_URL", "redis://127.0.0.1:1");
    quench_core::env::remove("CACHE_URL");
    quench_core::env::set("REDIS_CLUSTER", "TRUE");
    let err = CacheStore::from_env("svc").await.unwrap_err();
    assert!(err.to_string().contains("cluster"), "{err}");
    quench_core::env::remove("REDIS_CLUSTER");
    let err = CacheStore::from_env("svc").await.unwrap_err();
    assert!(err.to_string().contains("connection failed"), "{err}");

    if let Some(url) = std::env::var("CACHE_TEST_REDIS_URL")
        .ok()
        .filter(|u| !u.is_empty())
    {
        quench_core::env::set("CACHE_URL", &url);
        let store = CacheStore::from_env("svc").await.expect("connects");
        assert!(store.is_shared());
        assert_eq!(store.topology(), "a single Redis server");
    }
    for key in ["CACHE_URL", "REDIS_URL"] {
        quench_core::env::remove(key);
    }
}

quench_test::main!();
