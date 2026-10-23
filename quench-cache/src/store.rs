//! A cache you can point at Redis or keep in the process.
//!
//! [`DataCache`] lives inside one process, which is the right thing for
//! caching an outbound HTTP response and the wrong thing for anything that has
//! to be consistent across replicas: an entry invalidated on one pod stays
//! valid on every other until its TTL runs out. [`CacheStore`] is the shared
//! shape - the same calls against either backend - so that choice becomes
//! configuration rather than a rewrite.

use crate::CacheError;
use quench_serde::Value;
use std::sync::Arc;

#[cfg(feature = "redis")]
use quench_redis::{Redis, cmd};

/// Where cached values live.
#[derive(Clone, Debug)]
pub enum CacheStore {
    /// Per-process, backed by [`DataCache`]. Fast, and invisible to other
    /// replicas.
    InMemory(Arc<crate::data_cache::DataCache>),

    /// Shared, backed by Redis. Survives a restart and is visible estate-wide.
    #[cfg(feature = "redis")]
    Redis(RedisStore),
}

// Without the `redis` feature no variant awaits anything, but the signatures are
// the public API either way.
#[cfg_attr(
    not(feature = "redis"),
    allow(clippy::unused_async, clippy::unused_async_trait_impl)
)]
impl CacheStore {
    #[must_use]
    pub fn in_memory() -> Self {
        Self::InMemory(Arc::new(crate::data_cache::DataCache::new()))
    }

    /// Redis when `CACHE_URL` (or `REDIS_URL`) is set, in-process otherwise.
    ///
    /// A comma-separated URL is read as cluster seed nodes; `CACHE_CLUSTER=true`
    /// (or `REDIS_CLUSTER=true`) forces cluster mode when you only have one seed
    /// to hand.
    ///
    /// Without the `redis` feature a configured URL is an error rather than a
    /// silent downgrade: a deployment that asked for a shared cache and got a
    /// per-process one would be wrong in a way nobody notices until two
    /// replicas disagree.
    #[cfg_attr(not(feature = "redis"), allow(clippy::option_if_let_else))]
    pub async fn from_env(key_prefix: impl Into<String>) -> Result<Self, CacheError> {
        let url = ["CACHE_URL", "REDIS_URL"]
            .iter()
            .find_map(|key| std::env::var(key).ok())
            .filter(|value| !value.trim().is_empty());

        match url {
            None => Ok(Self::in_memory()),
            #[cfg(feature = "redis")]
            Some(url) => {
                let forced = ["CACHE_CLUSTER", "REDIS_CLUSTER"]
                    .iter()
                    .find_map(|key| std::env::var(key).ok())
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("true"));
                Ok(Self::Redis(
                    RedisStore::connect_with(&url, key_prefix, forced).await?,
                ))
            }
            #[cfg(not(feature = "redis"))]
            Some(url) => {
                let _ = key_prefix;
                Err(CacheError::Backend(format!(
                    "a cache URL is configured ({url}) but quench-cache was built \
                     without the `redis` feature"
                )))
            }
        }
    }

    pub async fn get(&self, key: &str) -> Result<Option<Value>, CacheError> {
        match self {
            Self::InMemory(cache) => Ok(cache.get(key)),
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.get(key).await,
        }
    }

    /// Stores `value`, expiring after `ttl_secs` when given.
    pub async fn set(
        &self,
        key: &str,
        value: Value,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                match ttl_secs {
                    Some(ttl) => cache.set_with_ttl(key.to_string(), value, ttl),
                    None => cache.set(key.to_string(), value),
                }
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.set(key, value, ttl_secs).await,
        }
    }

    /// Reads and removes in one step.
    ///
    /// This is what makes a single-use token single-use: two callers racing on
    /// the same key, exactly one gets the value.
    pub async fn take(&self, key: &str) -> Result<Option<Value>, CacheError> {
        match self {
            Self::InMemory(cache) => {
                let value = cache.get(key);
                let _ = cache.delete(key);
                Ok(value)
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.take(key).await,
        }
    }

    pub async fn remove(&self, key: &str) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                let _ = cache.delete(key);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.remove(key).await,
        }
    }

    /// Adds `member` to the set at `key`, refreshing the set's lifetime.
    ///
    /// A set rather than a value because the members arrive from concurrent
    /// writers - several sessions for one user - and read-modify-write over a
    /// JSON array would drop one of two simultaneous additions.
    pub async fn add_to_set(
        &self,
        key: &str,
        member: &str,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.add_to_set(key, member, ttl_secs);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.add_to_set(key, member, ttl_secs).await,
        }
    }

    /// Members of the set at `key`, in no particular order. A missing set reads
    /// as empty rather than as an error - to a caller they mean the same thing.
    pub async fn set_members(&self, key: &str) -> Result<Vec<String>, CacheError> {
        match self {
            Self::InMemory(cache) => Ok(cache.set_members(key)),
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.set_members(key).await,
        }
    }

    pub async fn remove_from_set(&self, key: &str, member: &str) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.remove_from_set(key, member);
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.remove_from_set(key, member).await,
        }
    }

    /// Drops everything this store owns. On Redis that is the configured key
    /// prefix only - never the whole database, which may not be yours.
    pub async fn clear(&self) -> Result<(), CacheError> {
        match self {
            Self::InMemory(cache) => {
                cache.clear();
                Ok(())
            }
            #[cfg(feature = "redis")]
            Self::Redis(store) => store.clear().await,
        }
    }

    /// Whether entries are visible to other processes. Worth checking before
    /// relying on this for anything that must be consistent estate-wide.
    #[must_use]
    pub const fn is_shared(&self) -> bool {
        match self {
            Self::InMemory(_) => false,
            #[cfg(feature = "redis")]
            Self::Redis(_) => true,
        }
    }

    /// What this store is talking to, for the startup log - an operator should
    /// be able to tell a clustered pod from a standalone one without guessing.
    #[must_use]
    pub const fn topology(&self) -> &'static str {
        match self {
            Self::InMemory(_) => "in-process memory",
            #[cfg(feature = "redis")]
            Self::Redis(store) if store.is_clustered() => "a Redis cluster",
            #[cfg(feature = "redis")]
            Self::Redis(_) => "a single Redis server",
        }
    }
}

#[cfg(feature = "redis")]
#[derive(Clone)]
pub struct RedisStore {
    redis: Redis,
    /// Namespaces every key, so several services can share one Redis without
    /// colliding, and `clear` has something to scope itself to.
    prefix: String,
}

#[cfg(feature = "redis")]
impl std::fmt::Debug for RedisStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisStore")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "redis")]
fn backend(err: &quench_redis::Error) -> CacheError {
    CacheError::Backend(err.to_string())
}

#[cfg(feature = "redis")]
impl RedisStore {
    /// Connects, reading a comma-separated `url` as cluster seed nodes.
    pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, CacheError> {
        Self::connect_with(url, prefix, false).await
    }

    /// Same, but `cluster` forces cluster mode even for a single seed - a
    /// cluster you reach through one address is indistinguishable from a
    /// standalone server by URL alone.
    pub async fn connect_with(
        url: &str,
        prefix: impl Into<String>,
        cluster: bool,
    ) -> Result<Self, CacheError> {
        let seeds: Vec<&str> = url
            .split(',')
            .map(str::trim)
            .filter(|seed| !seed.is_empty())
            .collect();

        if seeds.is_empty() {
            return Err(CacheError::Backend("no Redis URL configured".into()));
        }

        let redis = if cluster || seeds.len() > 1 {
            Redis::connect_cluster(&seeds).await.map_err(|err| {
                CacheError::Backend(format!("Redis cluster connection failed: {err}"))
            })?
        } else {
            Redis::connect(seeds[0])
                .await
                .map_err(|err| CacheError::Backend(format!("Redis connection failed: {err}")))?
        };

        Ok(Self {
            redis,
            prefix: prefix.into(),
        })
    }

    /// Whether this store is talking to a cluster.
    #[must_use]
    pub const fn is_clustered(&self) -> bool {
        self.redis.is_cluster()
    }

    fn qualified(&self, key: &str) -> String {
        if self.prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}:{key}", self.prefix)
        }
    }

    fn decode(raw: Option<String>) -> Result<Option<Value>, CacheError> {
        raw.map(|value| {
            quench_serde::json::from_str(&value)
                .map_err(|err| CacheError::SerializationError(err.to_string()))
        })
        .transpose()
    }

    pub async fn get(&self, key: &str) -> Result<Option<Value>, CacheError> {
        let raw: Option<String> = self
            .redis
            .exec(&cmd("GET").arg(&self.qualified(key)))
            .await
            .map_err(|err| backend(&err))?;
        Self::decode(raw)
    }

    pub async fn set(
        &self,
        key: &str,
        value: Value,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        let encoded = quench_serde::json::to_string(&value);
        let mut command = cmd("SET").arg(&self.qualified(key)).arg(&encoded);
        if let Some(ttl) = ttl_secs {
            command = command.arg("EX").arg(&ttl);
        }
        self.redis
            .exec::<()>(&command)
            .await
            .map_err(|err| backend(&err))
    }

    /// `GETDEL`: atomic on the server, so concurrent callers cannot both win.
    pub async fn take(&self, key: &str) -> Result<Option<Value>, CacheError> {
        let raw: Option<String> = self
            .redis
            .exec(&cmd("GETDEL").arg(&self.qualified(key)))
            .await
            .map_err(|err| backend(&err))?;
        Self::decode(raw)
    }

    pub async fn remove(&self, key: &str) -> Result<(), CacheError> {
        self.redis
            .exec::<()>(&cmd("DEL").arg(&self.qualified(key)))
            .await
            .map_err(|err| backend(&err))
    }

    /// `SADD`, then `EXPIRE` when a lifetime is given.
    ///
    /// Two commands rather than one, and not in a transaction: a set that ends
    /// up without its expiry is a key that outlives its usefulness, which is
    /// worse than nothing but not wrong. Both are single-key, so a cluster
    /// routes them to the same node.
    pub async fn add_to_set(
        &self,
        key: &str,
        member: &str,
        ttl_secs: Option<u64>,
    ) -> Result<(), CacheError> {
        let qualified = self.qualified(key);

        self.redis
            .exec::<()>(&cmd("SADD").arg(&qualified).arg(member))
            .await
            .map_err(|err| backend(&err))?;

        if let Some(ttl) = ttl_secs {
            self.redis
                .exec::<()>(&cmd("EXPIRE").arg(&qualified).arg(&ttl))
                .await
                .map_err(|err| backend(&err))?;
        }
        Ok(())
    }

    pub async fn set_members(&self, key: &str) -> Result<Vec<String>, CacheError> {
        self.redis
            .exec(&cmd("SMEMBERS").arg(&self.qualified(key)))
            .await
            .map_err(|err| backend(&err))
    }

    /// `SREM`. Redis drops a set once its last member leaves, so there is no
    /// empty key left behind to clean up.
    pub async fn remove_from_set(&self, key: &str, member: &str) -> Result<(), CacheError> {
        self.redis
            .exec::<()>(&cmd("SREM").arg(&self.qualified(key)).arg(member))
            .await
            .map_err(|err| backend(&err))
    }

    /// Deletes this store's keys with `SCAN`, which - unlike `KEYS` - does not
    /// block the server while it walks the keyspace.
    ///
    /// On a cluster this walks every primary in turn: `SCAN` only ever reports
    /// the keyspace of the node that answers it, and a cursor from one node
    /// means nothing to another.
    pub async fn clear(&self) -> Result<(), CacheError> {
        let pattern = self.qualified("*");

        if self.redis.is_cluster() {
            let primaries = self
                .redis
                .primaries()
                .await
                .map_err(|err| CacheError::Backend(format!("CLUSTER NODES failed: {err}")))?;
            for node in primaries {
                self.sweep(&pattern, Some(&node)).await?;
            }
            Ok(())
        } else {
            self.sweep(&pattern, None).await
        }
    }

    /// Walks `pattern` on one node and deletes what it finds.
    ///
    /// `node` picks the primary to scan; without it the command goes wherever
    /// the connection would normally send it, which is the whole keyspace on a
    /// standalone server.
    async fn sweep(&self, pattern: &str, node: Option<&str>) -> Result<(), CacheError> {
        let mut cursor: u64 = 0;

        loop {
            let scan = cmd("SCAN")
                .arg(&cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(&500);

            let (next, keys): (u64, Vec<String>) = match node {
                Some(node) => self.redis.exec_on(node, &scan).await,
                None => self.redis.exec(&scan).await,
            }
            .map_err(|err| backend(&err))?;

            // One key per `DEL`: on a cluster a multi-key delete is rejected
            // outright unless every key hashes to the same slot.
            for key in keys {
                self.redis
                    .exec::<()>(&cmd("DEL").arg(&key))
                    .await
                    .map_err(|err| backend(&err))?;
            }

            cursor = next;
            if cursor == 0 {
                return Ok(());
            }
        }
    }
}

/// Reads one `CLUSTER NODES` line, keeping healthy primaries.
#[cfg(feature = "redis")]
#[doc(hidden)]
#[must_use]
pub fn parse_primary(line: &str) -> Option<(String, u16)> {
    quench_redis::parse_primary(line)
}
