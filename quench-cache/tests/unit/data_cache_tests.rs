//! Unit tests for `data_cache.rs`.

use quench_cache::data_cache::*;

#[test]
fn test_set_and_get() {
    let cache = DataCache::new();
    let key = "user:123".to_string();
    let value = serde_json::json!({"id": 123, "name": "Alice"});

    cache.set(key.clone(), value.clone());
    let retrieved = cache.get(&key);

    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), value);
}

#[test]
fn test_ttl_expiration() {
    let cache = DataCache::new();
    let key = "temp_key".to_string();
    let value = serde_json::json!({"data": "temporary"});

    cache.set_with_ttl(key.clone(), value, 0);
    let retrieved = cache.get(&key);

    assert!(retrieved.is_none());
}

#[test]
fn test_delete() {
    let cache = DataCache::new();
    let key = "delete_me".to_string();
    cache.set(key.clone(), serde_json::json!({}));

    assert!(cache.has(&key));
    assert!(cache.delete(&key));
    assert!(!cache.has(&key));
}

#[test]
fn has_is_false_for_a_missing_key_and_delete_reports_whether_anything_was_removed() {
    let cache = DataCache::new();
    assert!(!cache.has("missing"));
    assert!(!cache.delete("missing"));
}

#[test]
fn a_value_with_no_ttl_never_expires() {
    let entry = CacheEntry {
        value: serde_json::json!("x"),
        timestamp: 0,
        ttl_secs: None,
    };
    assert!(!entry.is_expired());
}

#[test]
fn new_and_default_both_start_empty() {
    assert!(DataCache::new().is_empty());
    assert!(DataCache::default().is_empty());
}

#[test]
fn len_and_is_empty_track_the_entry_count() {
    let cache = DataCache::new();
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);

    cache.set("a".to_string(), serde_json::json!(1));
    cache.set("b".to_string(), serde_json::json!(2));
    assert_eq!(cache.len(), 2);
    assert!(!cache.is_empty());
}

#[test]
fn clear_removes_every_entry() {
    let cache = DataCache::new();
    cache.set("a".to_string(), serde_json::json!(1));
    cache.set("b".to_string(), serde_json::json!(2));
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn stats_reports_size_and_expired_count() {
    let cache = DataCache::new();
    cache.set("fresh".to_string(), serde_json::json!("abc"));
    cache.set_with_ttl("stale".to_string(), serde_json::json!("xyz"), 0);

    let stats = cache.stats();
    assert_eq!(stats.total_entries, 2);
    assert_eq!(stats.expired_entries, 1);
    assert!(stats.total_size_bytes > 0);
}

#[test]
fn cleanup_expired_removes_only_expired_entries_and_reports_how_many() {
    let cache = DataCache::new();
    cache.set("fresh".to_string(), serde_json::json!("abc"));
    cache.set_with_ttl("stale_one".to_string(), serde_json::json!("x"), 0);
    cache.set_with_ttl("stale_two".to_string(), serde_json::json!("y"), 0);

    let removed = cache.cleanup_expired();
    assert_eq!(removed, 2);
    assert_eq!(cache.len(), 1);
    assert!(cache.has("fresh"));
}

#[test]
fn add_to_set_creates_the_set_and_ignores_duplicate_members() {
    let cache = DataCache::new();
    cache.add_to_set("tags", "a", None);
    cache.add_to_set("tags", "b", None);
    cache.add_to_set("tags", "a", None);

    let mut members = cache.set_members("tags");
    members.sort();
    assert_eq!(members, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn add_to_set_replaces_an_expired_set_instead_of_appending_to_it() {
    let cache = DataCache::new();
    cache.add_to_set("tags", "old", Some(0));
    // The set above is already expired the instant it's read back.
    cache.add_to_set("tags", "new", None);

    let members = cache.set_members("tags");
    assert_eq!(members, vec!["new".to_string()]);
}

#[test]
fn add_to_set_replaces_a_non_array_value_at_the_same_key() {
    let cache = DataCache::new();
    cache.set("tags".to_string(), serde_json::json!("not-an-array"));
    cache.add_to_set("tags", "member", None);

    assert_eq!(cache.set_members("tags"), vec!["member".to_string()]);
}

#[test]
fn set_members_is_empty_for_a_missing_key() {
    let cache = DataCache::new();
    assert!(cache.set_members("missing").is_empty());
}

#[test]
fn remove_from_set_drops_the_key_once_the_last_member_leaves() {
    let cache = DataCache::new();
    cache.add_to_set("tags", "only", None);
    cache.remove_from_set("tags", "only");

    assert!(cache.set_members("tags").is_empty());
    assert!(!cache.has("tags"));
}

#[test]
fn remove_from_set_on_a_missing_key_or_member_is_a_no_op() {
    let cache = DataCache::new();
    cache.remove_from_set("missing", "member");

    cache.add_to_set("tags", "a", None);
    cache.remove_from_set("tags", "not-there");
    assert_eq!(cache.set_members("tags"), vec!["a".to_string()]);
}
