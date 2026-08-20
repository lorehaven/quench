//! Unit tests for `store.rs`.

use quench_cache::CacheStore;

#[tokio::test]
async fn an_in_memory_store_is_not_shared_and_reports_its_topology() {
    let store = CacheStore::in_memory();
    assert!(!store.is_shared());
    assert_eq!(store.topology(), "in-process memory");
}

#[tokio::test]
async fn an_in_memory_store_round_trips_a_value() {
    let store = CacheStore::in_memory();
    let value = serde_json::json!({"id": 1});

    assert!(store.get("key").await.unwrap().is_none());
    store.set("key", value.clone(), None).await.unwrap();
    assert_eq!(store.get("key").await.unwrap(), Some(value));
}

#[tokio::test]
async fn take_reads_and_removes_in_one_step() {
    let store = CacheStore::in_memory();
    store
        .set("key", serde_json::json!("v"), None)
        .await
        .unwrap();

    assert_eq!(
        store.take("key").await.unwrap(),
        Some(serde_json::json!("v"))
    );
    assert!(store.get("key").await.unwrap().is_none());
}

#[tokio::test]
async fn remove_deletes_a_value() {
    let store = CacheStore::in_memory();
    store
        .set("key", serde_json::json!("v"), None)
        .await
        .unwrap();
    store.remove("key").await.unwrap();
    assert!(store.get("key").await.unwrap().is_none());
}

#[tokio::test]
async fn sets_are_added_to_listed_and_removed_from() {
    let store = CacheStore::in_memory();
    store.add_to_set("tags", "a", None).await.unwrap();
    store.add_to_set("tags", "b", None).await.unwrap();

    let mut members = store.set_members("tags").await.unwrap();
    members.sort();
    assert_eq!(members, vec!["a".to_string(), "b".to_string()]);

    store.remove_from_set("tags", "a").await.unwrap();
    assert_eq!(store.set_members("tags").await.unwrap(), vec!["b"]);
}

#[tokio::test]
async fn clear_drops_every_entry() {
    let store = CacheStore::in_memory();
    store
        .set("key", serde_json::json!("v"), None)
        .await
        .unwrap();
    store.clear().await.unwrap();
    assert!(store.get("key").await.unwrap().is_none());
}

#[cfg(feature = "redis")]
mod parse_primary_tests {
    use quench_cache::store::parse_primary;

    #[test]
    fn keeps_healthy_primaries() {
        let line = "07c37d 127.0.0.1:7001@17001 myself,master - 0 1690000000000 1 connected 0-5460";
        assert_eq!(parse_primary(line), Some(("127.0.0.1".to_string(), 7001)));
    }

    #[test]
    fn skips_replicas() {
        let line = "e7d1ee 127.0.0.1:7004@17004 slave 07c37d 0 1690000000000 4 connected";
        assert_eq!(parse_primary(line), None);
    }

    #[test]
    fn skips_primaries_the_cluster_has_given_up_on() {
        let failed = "07c37d 127.0.0.1:7001@17001 master,fail - 0 1690000000000 1 disconnected";
        let suspect = "07c37d 127.0.0.1:7002@17002 master,fail? - 0 1690000000000 1 connected";
        assert_eq!(parse_primary(failed), None);
        assert_eq!(parse_primary(suspect), None);
    }

    /// Redis 7 appends a hostname and auxiliary fields to the address; the port we
    /// want is still the client port, not the cluster bus port after the `@`.
    #[test]
    fn reads_the_client_port_from_a_decorated_address() {
        let line =
            "07c37d 10.0.0.5:6379@16379,node-a.redis.svc master - 0 1690000000000 1 connected";
        assert_eq!(parse_primary(line), Some(("10.0.0.5".to_string(), 6379)));
    }

    #[test]
    fn ignores_lines_that_are_not_node_records() {
        assert_eq!(parse_primary(""), None);
        assert_eq!(parse_primary("07c37d 127.0.0.1:7001@17001"), None);
    }
}
