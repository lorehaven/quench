//! Exercises the Redis backend against a real server.
//!
//! Skipped unless `CACHE_TEST_REDIS_URL` is set, so the suite stays runnable
//! without infrastructure. The cluster tests want
//! `CACHE_TEST_REDIS_CLUSTER_URL` - a comma-separated list of seed nodes - and
//! are skipped separately, since a cluster is more than most machines keep
//! running.

#![cfg(feature = "redis")]

use quench_cache::CacheStore;
use quench_cache::store::RedisStore;
use quench_serde::json;

fn url() -> Option<String> {
    std::env::var("CACHE_TEST_REDIS_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

fn cluster_url() -> Option<String> {
    std::env::var("CACHE_TEST_REDIS_CLUSTER_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

async fn cluster_store(prefix: &str) -> Option<CacheStore> {
    let url = cluster_url()?;
    Some(CacheStore::Redis(
        RedisStore::connect(&url, prefix)
            .await
            .expect("connect to the cluster"),
    ))
}

#[tokio::test]
async fn values_round_trip_and_expire() {
    let Some(url) = url() else { return };
    let store = CacheStore::Redis(
        RedisStore::connect(&url, "forge-test")
            .await
            .expect("connect"),
    );
    store.clear().await.expect("clear");

    assert!(
        store.is_shared(),
        "a Redis-backed store is visible estate-wide"
    );
    assert_eq!(store.get("missing").await.expect("get"), None);

    store
        .set("greeting", json!({"hello": "world"}), None)
        .await
        .expect("set");
    assert_eq!(
        store.get("greeting").await.expect("get"),
        Some(json!({"hello": "world"}))
    );

    store.remove("greeting").await.expect("remove");
    assert_eq!(store.get("greeting").await.expect("get"), None);

    // One second is the shortest TTL `SET EX` can express.
    store.set("brief", json!(1), Some(1)).await.expect("set");
    assert!(store.get("brief").await.expect("get").is_some());
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    assert_eq!(store.get("brief").await.expect("get"), None, "TTL expired");
}

/// `clear` must take this store's keys and leave everything else alone - the
/// Redis may not be ours exclusively.
#[tokio::test]
async fn clear_is_scoped_to_the_prefix() {
    let Some(url) = url() else { return };
    let ours = CacheStore::Redis(
        RedisStore::connect(&url, "forge-scoped")
            .await
            .expect("connect"),
    );
    let theirs = CacheStore::Redis(
        RedisStore::connect(&url, "somebody-else")
            .await
            .expect("connect"),
    );

    ours.set("a", json!(1), None).await.expect("set");
    theirs.set("a", json!(2), None).await.expect("set");

    ours.clear().await.expect("clear");

    assert_eq!(ours.get("a").await.expect("get"), None);
    assert_eq!(theirs.get("a").await.expect("get"), Some(json!(2)));
    theirs.clear().await.expect("clear");
}

/// The same round trip against a cluster. Keys spread across slots, so this is
/// really a test that the client follows `MOVED` rather than failing on it.
#[tokio::test]
async fn cluster_values_round_trip_and_expire() {
    let Some(store) = cluster_store("forge-cluster").await else {
        return;
    };
    store.clear().await.expect("clear");

    assert!(store.is_shared());
    assert_eq!(store.get("missing").await.expect("get"), None);

    store
        .set("greeting", json!({"hello": "world"}), None)
        .await
        .expect("set");
    assert_eq!(
        store.get("greeting").await.expect("get"),
        Some(json!({"hello": "world"}))
    );

    // `take` is what makes a refresh token single-use, and it has to stay
    // atomic on whichever node owns the slot.
    assert_eq!(
        store.take("greeting").await.expect("take"),
        Some(json!({"hello": "world"}))
    );
    assert_eq!(store.take("greeting").await.expect("take"), None);

    store.set("brief", json!(1), Some(1)).await.expect("set");
    assert!(store.get("brief").await.expect("get").is_some());
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    assert_eq!(store.get("brief").await.expect("get"), None, "TTL expired");
}

/// `clear` has to walk every primary: with enough keys they are certain to
/// land on more than one node, and a single-node `SCAN` would miss most of
/// them while still reporting success.
#[tokio::test]
async fn cluster_clear_sweeps_every_primary() {
    let Some(ours) = cluster_store("forge-cluster-clear").await else {
        return;
    };
    let Some(theirs) = cluster_store("somebody-else-cluster").await else {
        return;
    };
    ours.clear().await.expect("clear");
    theirs.clear().await.expect("clear");

    let keys: Vec<String> = (0..64).map(|n| format!("key-{n}")).collect();
    for key in &keys {
        ours.set(key, json!(1), None).await.expect("set");
    }
    theirs.set("key-0", json!(2), None).await.expect("set");

    ours.clear().await.expect("clear");

    for key in &keys {
        assert_eq!(
            ours.get(key).await.expect("get"),
            None,
            "{key} survived the sweep"
        );
    }
    assert_eq!(
        theirs.get("key-0").await.expect("get"),
        Some(json!(2)),
        "another prefix was swept away"
    );
    theirs.clear().await.expect("clear");
}

async fn single(prefix: &str) -> Option<CacheStore> {
    let url = url()?;
    Some(CacheStore::Redis(
        RedisStore::connect(&url, prefix).await.expect("connect"),
    ))
}

#[tokio::test]
async fn take_is_atomic_and_single_use() {
    let Some(store) = single("forge-take").await else {
        return;
    };
    store.clear().await.expect("clear");
    store.set("token", json!("abc"), None).await.expect("set");
    assert_eq!(store.take("token").await.expect("take"), Some(json!("abc")));
    assert_eq!(store.take("token").await.expect("take"), None);
    store
        .remove("never-existed")
        .await
        .expect("removing a missing key is fine");
}

#[tokio::test]
async fn sets_hold_members_and_expire() {
    let Some(store) = single("forge-sets").await else {
        return;
    };
    store.clear().await.expect("clear");
    store.add_to_set("members", "a", None).await.expect("add");
    store
        .add_to_set("members", "b", Some(100))
        .await
        .expect("add with ttl");
    store
        .add_to_set("members", "a", None)
        .await
        .expect("a duplicate is ignored");
    let mut members = store.set_members("members").await.expect("members");
    members.sort();
    assert_eq!(members, ["a", "b"]);
    store.remove_from_set("members", "a").await.expect("remove");
    assert_eq!(store.set_members("members").await.expect("members"), ["b"]);
    store.remove_from_set("members", "b").await.expect("remove");
    assert!(
        store
            .set_members("members")
            .await
            .expect("members")
            .is_empty()
    );
    assert!(
        store
            .set_members("no-such-set")
            .await
            .expect("members")
            .is_empty()
    );

    store
        .add_to_set("short-lived", "x", Some(1))
        .await
        .expect("add");
    tokio::time::sleep(std::time::Duration::from_millis(1400)).await;
    assert!(
        store
            .set_members("short-lived")
            .await
            .expect("members")
            .is_empty()
    );
}

#[tokio::test]
async fn clear_walks_many_pages() {
    let Some(store) = single("forge-many").await else {
        return;
    };
    store.clear().await.expect("clear");
    for n in 0..1200 {
        store
            .set(&format!("k{n}"), json!(n), None)
            .await
            .expect("set");
    }
    store.clear().await.expect("clear");
    assert_eq!(store.get("k0").await.expect("get"), None);
    assert_eq!(store.get("k1199").await.expect("get"), None);
}

#[tokio::test]
async fn bad_stored_json_is_a_serialization_error() {
    let Some(url) = url() else { return };
    let store = RedisStore::connect(&url, "forge-corrupt")
        .await
        .expect("connect");
    // Written behind the store's back, the way a foreign writer or an old format would.
    let redis = quench_redis::Redis::connect(&url).await.expect("connect");
    redis
        .exec::<()>(
            &quench_redis::cmd("SET")
                .arg("forge-corrupt:bad")
                .arg("{not json"),
        )
        .await
        .expect("set");
    let err = store.get("bad").await.expect_err("unparseable");
    assert!(
        matches!(err, quench_cache::CacheError::SerializationError(_)),
        "{err}"
    );
    let err = store.take("bad").await.expect_err("unparseable");
    assert!(
        matches!(err, quench_cache::CacheError::SerializationError(_)),
        "{err}"
    );
    redis
        .exec::<()>(
            &quench_redis::cmd("SET")
                .arg("forge-corrupt:bad")
                .arg("{not json"),
        )
        .await
        .expect("set again; take removed it");
    // A string where a set is expected is a server-side type error, surfaced as a backend error.
    let err = store.set_members("bad").await.expect_err("wrong type");
    assert!(matches!(err, quench_cache::CacheError::Backend(_)), "{err}");
    assert!(store.add_to_set("bad", "m", Some(10)).await.is_err());
    assert!(store.remove_from_set("bad", "m").await.is_err());
    store.remove("bad").await.expect("remove");
}

#[tokio::test]
async fn connecting_validates_its_input() {
    assert!(
        matches!(RedisStore::connect("  ,  ", "p").await, Err(quench_cache::CacheError::Backend(m)) if m.contains("no Redis URL"))
    );
    assert!(
        matches!(RedisStore::connect("http://nope", "p").await, Err(quench_cache::CacheError::Backend(m)) if m.contains("connection failed"))
    );
    assert!(matches!(
        RedisStore::connect("redis://127.0.0.1:1", "p").await,
        Err(quench_cache::CacheError::Backend(_))
    ));
    assert!(
        matches!(RedisStore::connect_with("redis://127.0.0.1:1", "p", true).await, Err(quench_cache::CacheError::Backend(m)) if m.contains("cluster"))
    );
    assert!(
        matches!(RedisStore::connect("redis://127.0.0.1:1,redis://127.0.0.1:2", "p").await, Err(quench_cache::CacheError::Backend(m)) if m.contains("cluster"))
    );
}

#[tokio::test]
async fn store_reports_what_it_is() {
    let Some(url) = url() else { return };
    let store = RedisStore::connect(&url, "forge-info")
        .await
        .expect("connect");
    assert!(!store.is_clustered());
    assert!(format!("{store:?}").contains("forge-info"));
    let wrapped = CacheStore::Redis(store);
    assert!(wrapped.is_shared());
    assert_eq!(wrapped.topology(), "a single Redis server");
    let unprefixed = CacheStore::Redis(RedisStore::connect(&url, "").await.expect("connect"));
    unprefixed
        .set("forge-info-bare", json!(1), None)
        .await
        .expect("set");
    assert_eq!(
        unprefixed.get("forge-info-bare").await.expect("get"),
        Some(json!(1))
    );
    unprefixed.remove("forge-info-bare").await.expect("remove");
}
