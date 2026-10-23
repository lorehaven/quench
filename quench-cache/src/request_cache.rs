use quench_core::map::ShardedMap;
use quench_serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached HTTP response or request result with TTL
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResponse {
    pub content: String,
    pub model: String,
    pub timestamp: u64,
    pub tokens_used: Option<u32>,
}

/// Request/response cache with TTL (24 hours default)
#[derive(Debug)]
pub struct RequestCache {
    cache: Arc<ShardedMap<String, CachedResponse>>,
    ttl_secs: u64,
}

impl RequestCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Arc::new(ShardedMap::new()),
            ttl_secs: 24 * 60 * 60, // 24 hours
        }
    }

    #[must_use]
    pub fn with_ttl(ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(ShardedMap::new()),
            ttl_secs,
        }
    }

    /// Generate cache key from inputs
    #[must_use]
    pub fn generate_key(parts: &[&str]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        parts.join("-").hash(&mut hasher);
        format!("cache_{}", hasher.finish())
    }

    /// Get cached response if valid and not expired
    #[must_use]
    pub fn get(&self, key: &str) -> Option<CachedResponse> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ttl = self.ttl_secs;

        let entry = self.cache.get(key)?;
        if now.saturating_sub(entry.timestamp) < ttl {
            return Some(entry);
        }
        // Expired, remove it (unless a writer replaced it with a fresh one).
        self.cache
            .remove_if(key, |held| now.saturating_sub(held.timestamp) >= ttl);
        None
    }

    /// Store response in cache
    pub fn set(&self, key: String, response: CachedResponse) {
        self.cache.insert(key, response);
    }

    /// Check if key exists in cache
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.cache.contains_key(key)
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.cache.clear();
    }

    /// Get cache statistics
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0;
        let mut expired_count = 0;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.cache.for_each(|_, entry| {
            let age = now.saturating_sub(entry.timestamp);
            if age >= self.ttl_secs {
                expired_count += 1;
            }
            total_size += entry.content.len();
        });

        CacheStats {
            total_entries: self.cache.len(),
            expired_entries: expired_count,
            total_size_bytes: total_size,
        }
    }

    /// Remove expired entries from cache
    #[must_use]
    pub fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut removed = 0;
        self.cache.retain(|_, entry| {
            let age = now.saturating_sub(entry.timestamp);
            if age >= self.ttl_secs {
                removed += 1;
                false
            } else {
                true
            }
        });

        removed
    }
}

impl Default for RequestCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub total_size_bytes: usize,
}
