//! A type-keyed map for per-request data a middleware hands to a handler.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

/// At most one value per type.
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `value`, returning the previous value of the same type.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast().ok().map(|boxed| *boxed))
    }

    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut())
    }

    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|old| old.downcast().ok().map(|boxed| *boxed))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions")
            .field("len", &self.map.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Claims(&'static str);

    #[quench_test::test]
    fn stores_one_value_per_type() {
        let mut ext = Extensions::new();
        assert!(ext.is_empty());
        assert_eq!(ext.insert(Claims("a")), None);
        assert_eq!(ext.insert(Claims("b")), Some(Claims("a")));
        ext.insert(7u32);
        assert_eq!(ext.len(), 2);
        assert_eq!(ext.get::<Claims>(), Some(&Claims("b")));
        *ext.get_mut::<u32>().unwrap() += 1;
        assert_eq!(ext.get::<u32>(), Some(&8));
        assert_eq!(ext.get::<String>(), None);
        assert_eq!(ext.get_mut::<String>(), None);
        assert_eq!(ext.remove::<u32>(), Some(8));
        assert_eq!(ext.remove::<u32>(), None);
        assert!(format!("{ext:?}").contains("len"));
        ext.clear();
        assert!(ext.is_empty());
    }
}
