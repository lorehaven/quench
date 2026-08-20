//! Unit tests for `request_cache.rs`.

use quench_cache::request_cache::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn test_cache_key_generation() {
    let key1 = RequestCache::generate_key(&["conv1", "hello", "model1"]);
    let key2 = RequestCache::generate_key(&["conv1", "hello", "model1"]);
    assert_eq!(key1, key2);

    let key3 = RequestCache::generate_key(&["conv1", "different", "model1"]);
    assert_ne!(key1, key3);
}

#[test]
fn test_cache_set_and_get() {
    let cache = RequestCache::new();
    let key = "test_key".to_string();
    let response = CachedResponse {
        content: "Test response".to_string(),
        model: "test-model".to_string(),
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        tokens_used: Some(100),
    };

    cache.set(key.clone(), response.clone());
    let retrieved = cache.get(&key);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().content, "Test response");
}

fn response(content: &str, timestamp: u64) -> CachedResponse {
    CachedResponse {
        content: content.to_string(),
        model: "test-model".to_string(),
        timestamp,
        tokens_used: None,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn get_returns_none_for_a_missing_key() {
    let cache = RequestCache::new();
    assert!(cache.get("missing").is_none());
}

#[test]
fn an_entry_older_than_the_ttl_is_treated_as_a_miss_and_evicted() {
    let cache = RequestCache::with_ttl(0);
    cache.set("key".to_string(), response("x", now()));

    assert!(cache.get("key").is_none());
    assert!(!cache.has("key"));
}

#[test]
fn has_reflects_whether_the_key_is_present_regardless_of_expiry() {
    let cache = RequestCache::new();
    assert!(!cache.has("key"));
    cache.set("key".to_string(), response("x", now()));
    assert!(cache.has("key"));
}

#[test]
fn clear_empties_the_cache() {
    let cache = RequestCache::new();
    cache.set("a".to_string(), response("x", now()));
    cache.set("b".to_string(), response("y", now()));
    cache.clear();

    assert!(!cache.has("a"));
    assert!(!cache.has("b"));
}

#[test]
fn stats_counts_entries_and_expired_entries_and_sums_content_length() {
    let cache = RequestCache::with_ttl(1000);
    cache.set("fresh".to_string(), response("abc", now()));
    cache.set("stale".to_string(), response("xyz", now() - 2000));

    let stats = cache.stats();
    assert_eq!(stats.total_entries, 2);
    assert_eq!(stats.expired_entries, 1);
    assert_eq!(stats.total_size_bytes, 6);
}

#[test]
fn cleanup_expired_removes_only_expired_entries_and_reports_how_many() {
    let cache = RequestCache::with_ttl(1000);
    cache.set("fresh".to_string(), response("abc", now()));
    cache.set("stale_one".to_string(), response("x", now() - 2000));
    cache.set("stale_two".to_string(), response("y", now() - 2000));

    let removed = cache.cleanup_expired();
    assert_eq!(removed, 2);
    assert!(cache.has("fresh"));
    assert!(!cache.has("stale_one"));
    assert!(!cache.has("stale_two"));
}

#[test]
fn new_defaults_to_a_twenty_four_hour_ttl_and_default_matches_new() {
    let fresh = RequestCache::new();
    fresh.set("key".to_string(), response("x", now() - 60));
    assert!(fresh.get("key").is_some());

    let defaulted = RequestCache::default();
    defaulted.set("key".to_string(), response("x", now() - 60));
    assert!(defaulted.get("key").is_some());
}
