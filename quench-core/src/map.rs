//! A concurrent map: a fixed number of independently locked shards.
//!
//! Closure-based access (`get`, `with`, `update`) instead of guards: a guard
//! held across another call on the same map is the classic way to deadlock a
//! sharded map, and the closure shape makes that impossible to write.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::sync::{Mutex, MutexGuard};

const SHARDS: usize = 16;

pub struct ShardedMap<K, V> {
    shards: [Mutex<HashMap<K, V>>; SHARDS],
    hasher: RandomState,
}

impl<K, V> Default for ShardedMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> fmt::Debug for ShardedMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShardedMap").finish_non_exhaustive()
    }
}

impl<K, V> ShardedMap<K, V> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| Mutex::new(HashMap::new())),
            hasher: RandomState::new(),
        }
    }

    /// Number of entries (a snapshot: other threads may be changing it).
    pub fn len(&self) -> usize {
        self.shards.iter().map(|shard| lock(shard).len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|shard| lock(shard).is_empty())
    }

    pub fn clear(&self) {
        for shard in &self.shards {
            lock(shard).clear();
        }
    }

    /// Keeps the entries `keep` returns `true` for. Shards are visited one at a
    /// time, so the result is not a single atomic snapshot.
    pub fn retain(&self, mut keep: impl FnMut(&K, &mut V) -> bool) {
        for shard in &self.shards {
            lock(shard).retain(|k, v| keep(k, v));
        }
    }

    /// Calls `visit` for every entry. Same consistency as [`retain`](Self::retain).
    pub fn for_each(&self, mut visit: impl FnMut(&K, &V)) {
        for shard in &self.shards {
            for (k, v) in lock(shard).iter() {
                visit(k, v);
            }
        }
    }
}

impl<K: Hash + Eq, V> ShardedMap<K, V> {
    fn shard<Q: Hash + ?Sized>(&self, key: &Q) -> MutexGuard<'_, HashMap<K, V>> {
        let index = usize::try_from(self.hasher.hash_one(key) % SHARDS as u64).unwrap_or(0);
        lock(&self.shards[index])
    }

    /// Stores `value`, returning what it replaced.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        self.shard(&key).insert(key, value)
    }

    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.shard(key).remove(key)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.shard(key).contains_key(key)
    }

    /// Runs `read` on the value, if present. The shard stays locked for the
    /// duration, so keep it short and do not call back into this map.
    pub fn with<Q, R>(&self, key: &Q, read: impl FnOnce(&V) -> R) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.shard(key).get(key).map(read)
    }

    /// Atomically inserts `default()` when the key is absent, then runs
    /// `change` on the value. This is the entry API: two threads updating the
    /// same key cannot lose each other's change.
    pub fn update<R>(
        &self,
        key: K,
        default: impl FnOnce() -> V,
        change: impl FnOnce(&mut V) -> R,
    ) -> R {
        let mut shard = self.shard(&key);
        change(shard.entry(key).or_insert_with(default))
    }

    /// Runs `change` on the value only if it exists.
    pub fn update_existing<Q, R>(&self, key: &Q, change: impl FnOnce(&mut V) -> R) -> Option<R>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.shard(key).get_mut(key).map(change)
    }

    /// Removes the entry when `remove_if` says so; returns whether it did.
    pub fn remove_if<Q>(&self, key: &Q, remove_if: impl FnOnce(&V) -> bool) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut shard = self.shard(key);
        if shard.get(key).is_some_and(remove_if) {
            shard.remove(key);
            true
        } else {
            false
        }
    }
}

impl<K: Hash + Eq, V: Clone> ShardedMap<K, V> {
    /// A clone of the value.
    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.with(key, V::clone)
    }
}

/// A poisoned shard still holds consistent data (every operation is a single
/// map call), so poisoning is ignored rather than propagated as a panic.
fn lock<K, V>(shard: &Mutex<HashMap<K, V>>) -> MutexGuard<'_, HashMap<K, V>> {
    shard
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn basic_operations() {
        let map: ShardedMap<String, i32> = ShardedMap::default();
        assert!(map.is_empty());
        assert_eq!(map.insert("a".into(), 1), None);
        assert_eq!(map.insert("a".into(), 2), Some(1));
        assert_eq!(map.get(&"a".to_string()), Some(2));
        assert_eq!(map.with(&"a".to_string(), |v| v * 10), Some(20));
        assert!(map.contains_key(&"a".to_string()));
        assert_eq!(map.len(), 1);
        assert_eq!(map.remove(&"a".to_string()), Some(2));
        assert_eq!(map.remove(&"a".to_string()), None);
        assert_eq!(map.get(&"a".to_string()), None);
        assert!(map.is_empty());
        assert!(format!("{map:?}").contains("ShardedMap"));
    }

    #[test]
    fn lookups_borrow_the_key() {
        let map: ShardedMap<String, u8> = ShardedMap::new();
        map.insert("owned".to_string(), 1);
        // &str finds a String key without allocating one
        assert_eq!(map.get("owned"), Some(1));
        assert!(map.contains_key("owned"));
        assert_eq!(map.with("owned", |v| *v + 1), Some(2));
        assert_eq!(
            map.update_existing("owned", |v| {
                *v += 5;
                *v
            }),
            Some(6)
        );
        assert!(map.remove_if("owned", |v| *v == 6));
        assert_eq!(map.remove("owned"), None);
    }

    #[test]
    fn spreads_many_keys_and_clears() {
        let map = ShardedMap::new();
        for i in 0..1_000 {
            map.insert(i, i * 2);
        }
        assert_eq!(map.len(), 1_000);
        let mut sum = 0;
        map.for_each(|_, v| sum += v);
        assert_eq!(sum, (0..1_000).map(|i| i * 2).sum::<i32>());
        map.clear();
        assert!(map.is_empty());
    }

    #[test]
    fn retain_filters_and_can_mutate() {
        let map = ShardedMap::new();
        for i in 0..100 {
            map.insert(i, i);
        }
        map.retain(|k, v| {
            *v += 1;
            k % 2 == 0
        });
        assert_eq!(map.len(), 50);
        assert_eq!(map.get(&4), Some(5));
        assert_eq!(map.get(&5), None);
    }

    #[test]
    fn update_inserts_then_changes() {
        let map: ShardedMap<&str, Vec<u8>> = ShardedMap::new();
        let len = map.update("k", Vec::new, |v| {
            v.push(1);
            v.len()
        });
        assert_eq!(len, 1);
        let len = map.update(
            "k",
            || unreachable!("exists"),
            |v| {
                v.push(2);
                v.len()
            },
        );
        assert_eq!(len, 2);
        assert_eq!(map.update_existing(&"k", |v| v.len()), Some(2));
        assert_eq!(map.update_existing(&"missing", |v| v.len()), None);
    }

    #[test]
    fn remove_if_checks_the_value() {
        let map = ShardedMap::new();
        map.insert("old", 1);
        map.insert("new", 9);
        assert!(map.remove_if(&"old", |v| *v < 5));
        assert!(!map.remove_if(&"new", |v| *v < 5));
        assert!(!map.remove_if(&"gone", |_| true));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn concurrent_updates_are_not_lost() {
        let map: Arc<ShardedMap<&str, u64>> = Arc::new(ShardedMap::new());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let map = Arc::clone(&map);
                std::thread::spawn(move || {
                    for _ in 0..1_000 {
                        map.update("counter", || 0, |n| *n += 1);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(map.get(&"counter"), Some(8_000));
    }

    #[test]
    fn survives_a_panicking_writer() {
        let map: Arc<ShardedMap<i32, i32>> = Arc::new(ShardedMap::new());
        map.insert(1, 1);
        let m = Arc::clone(&map);
        let _ = std::thread::spawn(move || {
            m.update(1, || 0, |_| panic!("boom"));
        })
        .join();
        assert_eq!(map.get(&1), Some(1));
        map.insert(1, 2);
        assert_eq!(map.get(&1), Some(2));
    }
}
