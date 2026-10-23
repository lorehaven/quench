//! An insertion-ordered string-keyed map: the `Object` of a [`Value`](crate::Value).
//!
//! Order is part of the data. A struct serialised to JSON keeps its field
//! order, and re-serialising a parsed document does not shuffle it.

use std::collections::HashMap;
use std::fmt;

use crate::Value;

#[derive(Clone, Default)]
pub struct Map {
    entries: Vec<(String, Value)>,
    /// Position of each key in `entries`.
    index: HashMap<String, usize>,
}

impl Map {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        let i = *self.index.get(key)?;
        Some(&mut self.entries[i].1)
    }

    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Stores `value`; an existing key keeps its position and gives back the old value.
    pub fn insert(&mut self, key: impl Into<String>, value: Value) -> Option<Value> {
        let key = key.into();
        if let Some(&i) = self.index.get(&key) {
            return Some(std::mem::replace(&mut self.entries[i].1, value));
        }
        self.index.insert(key.clone(), self.entries.len());
        self.entries.push((key, value));
        None
    }

    /// Removes a key, keeping the order of the rest.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let i = self.index.remove(key)?;
        let (_, value) = self.entries.remove(i);
        for position in self.index.values_mut().filter(|p| **p > i) {
            *position -= 1;
        }
        Some(value)
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&String, &Value)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    #[must_use]
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    #[must_use]
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Value> {
        self.entries.iter_mut().map(|(_, v)| v)
    }
}

impl PartialEq for Map {
    /// Same keys with equal values, regardless of order.
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl fmt::Debug for Map {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K: Into<String>> FromIterator<(K, Value)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, Value)>>(iter: I) -> Self {
        let mut map = Self::new();
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}

impl IntoIterator for Map {
    type Item = (String, Value);
    type IntoIter = std::vec::IntoIter<(String, Value)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a> IntoIterator for &'a Map {
    type Item = (&'a String, &'a Value);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, Value)>,
        fn(&'a (String, Value)) -> (&'a String, &'a Value),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(k, v)| (k, v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(i: u64) -> Value {
        Value::UInt(i)
    }

    #[test]
    fn insertion_order_is_kept() {
        let mut map = Map::new();
        map.insert("z", n(1));
        map.insert("a", n(2));
        map.insert("m", n(3));
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["z", "a", "m"]
        );
        assert_eq!(
            map.values().cloned().collect::<Vec<_>>(),
            [n(1), n(2), n(3)]
        );
        assert_eq!(map.len(), 3);
        assert!(!map.is_empty());
    }

    #[test]
    fn replacing_keeps_the_position() {
        let mut map = Map::new();
        map.insert("a", n(1));
        map.insert("b", n(2));
        assert_eq!(map.insert("a", n(9)), Some(n(1)));
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(map.get("a"), Some(&n(9)));
    }

    #[test]
    fn remove_shifts_later_entries() {
        let mut map: Map = [("a", n(1)), ("b", n(2)), ("c", n(3)), ("d", n(4))]
            .into_iter()
            .collect();
        assert_eq!(map.remove("b"), Some(n(2)));
        assert_eq!(map.remove("b"), None);
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["a", "c", "d"]
        );
        // lookups after the shift still land on the right entries
        assert_eq!(map.get("c"), Some(&n(3)));
        assert_eq!(map.get("d"), Some(&n(4)));
        assert_eq!(map.get("a"), Some(&n(1)));
        map.insert("e", n(5));
        assert_eq!(map.get("e"), Some(&n(5)));
        assert!(map.contains_key("e") && !map.contains_key("b"));
    }

    #[test]
    fn mutation_in_place() {
        let mut map = Map::new();
        map.insert("a", n(1));
        *map.get_mut("a").unwrap() = n(2);
        assert_eq!(map.get("a"), Some(&n(2)));
        assert!(map.get_mut("missing").is_none());
        for v in map.values_mut() {
            *v = n(3);
        }
        assert_eq!(map.get("a"), Some(&n(3)));
    }

    #[test]
    fn equality_ignores_order() {
        let a: Map = [("x", n(1)), ("y", n(2))].into_iter().collect();
        let b: Map = [("y", n(2)), ("x", n(1))].into_iter().collect();
        let c: Map = [("x", n(1)), ("y", n(3))].into_iter().collect();
        let d: Map = std::iter::once(("x", n(1))).collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn iteration_forms() {
        let map: Map = [("a", n(1)), ("b", n(2))].into_iter().collect();
        assert_eq!(map.iter().len(), 2);
        let borrowed: Vec<_> = (&map).into_iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(borrowed, ["a", "b"]);
        let owned: Vec<_> = map.clone().into_iter().collect();
        assert_eq!(owned, [("a".to_string(), n(1)), ("b".to_string(), n(2))]);
        assert_eq!(format!("{map:?}"), "{\"a\": UInt(1), \"b\": UInt(2)}");
    }
}
