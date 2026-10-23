//! A Redis client: RESP2/RESP3, pooled connections that reconnect on demand,
//! pipelines, and Redis Cluster routing.
//!
//! ```no_run
//! # async fn demo() -> Result<(), quench_redis::Error> {
//! use quench_redis::{Redis, cmd};
//! let redis = Redis::connect("redis://localhost:6379").await?;
//! redis.exec::<()>(&cmd("SET").arg("greeting").arg("hello")).await?;
//! let greeting: Option<String> = redis.exec(&cmd("GET").arg("greeting")).await?;
//! # Ok(()) }
//! ```

mod cluster;
mod cmd;
mod config;
mod conn;
mod error;
mod pool;
pub mod resp;

pub use cluster::{ClusterClient, SLOTS, SlotMap, crc16, key_slot, parse_primary};
pub use cmd::{Cmd, FromValue, ToArg, cmd};
pub use config::Config;
pub use conn::{Connection, Stream};
pub use error::{Error, Result, ServerError};
pub use pool::{Client, PoolOptions, PooledConnection};
pub use resp::Value;

/// One server or a whole cluster, behind one interface.
#[derive(Debug, Clone)]
pub enum Redis {
    Single(Client),
    Cluster(ClusterClient),
}

impl Redis {
    /// Connects to one server.
    ///
    /// # Errors
    ///
    /// See [`Client::connect`].
    pub async fn connect(url: &str) -> Result<Self> {
        Client::connect(url).await.map(Self::Single)
    }

    /// Connects to a cluster through its seed nodes.
    ///
    /// # Errors
    ///
    /// See [`ClusterClient::connect`].
    pub async fn connect_cluster(urls: &[&str]) -> Result<Self> {
        ClusterClient::connect(urls).await.map(Self::Cluster)
    }

    #[must_use]
    pub const fn is_cluster(&self) -> bool {
        matches!(self, Self::Cluster(_))
    }

    /// Runs a command (routed by its key on a cluster) and converts the reply.
    ///
    /// # Errors
    ///
    /// Transport errors, the server's error reply, or a reply that does not fit `T`.
    pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T> {
        match self {
            Self::Single(client) => client.exec(command).await,
            Self::Cluster(cluster) => cluster.exec(command).await,
        }
    }

    /// Runs a command on one node of a cluster; on a single server the address is ignored.
    ///
    /// # Errors
    ///
    /// As [`exec`](Self::exec).
    pub async fn exec_on<T: FromValue>(&self, address: &str, command: &Cmd) -> Result<T> {
        match self {
            Self::Single(client) => client.exec(command).await,
            Self::Cluster(cluster) => cluster.exec_on(address, command).await,
        }
    }

    /// The cluster's healthy primaries from `CLUSTER NODES` (`host:port`); empty for a single server.
    ///
    /// # Errors
    ///
    /// As [`exec`](Self::exec).
    pub async fn primaries(&self) -> Result<Vec<String>> {
        let Self::Cluster(cluster) = self else {
            return Ok(Vec::new());
        };
        let Some(any) = cluster.nodes().into_iter().next() else {
            return Err(Error::Cluster("the topology is empty".into()));
        };
        let raw: String = cluster.exec_on(&any, &cmd("CLUSTER").arg("NODES")).await?;
        Ok(raw
            .lines()
            .filter_map(parse_primary)
            .map(|(host, port)| {
                if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                }
            })
            .collect())
    }
}
