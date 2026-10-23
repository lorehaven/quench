//! A connection pool: at most `max_size` connections, reused across callers,
//! with health checks on checkout and limits on how long one may live.

use crate::config::Config;
use crate::conn::{Connection, QueryResult, Transaction};
use crate::error::{Error, Result};
use crate::row::Row;
use crate::types::ToSql;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

/// Pool limits and timeouts.
#[derive(Debug, Clone)]
pub struct PoolOptions {
    /// Most connections open at once (default 5).
    pub max_size: usize,
    /// How long [`Pool::get`] waits for a free connection (default 30 s).
    pub acquire_timeout: Duration,
    /// Idle connections older than this are closed instead of reused (default 10 minutes).
    pub idle_timeout: Duration,
    /// Connections older than this are closed when returned (default 30 minutes).
    pub max_lifetime: Duration,
    /// A connection idle for longer than this is checked with `SELECT 1` before being handed out (default 30 s).
    pub test_after_idle: Duration,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            max_size: 5,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
            max_lifetime: Duration::from_mins(30),
            test_after_idle: Duration::from_secs(30),
        }
    }
}

struct Inner {
    config: Config,
    options: PoolOptions,
    idle: Mutex<VecDeque<Connection>>,
    permits: Arc<Semaphore>,
}

/// A cheap-to-clone handle to a pool.
#[derive(Clone)]
pub struct Pool {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("max_size", &self.inner.options.max_size)
            .field("idle", &self.idle_count())
            .finish_non_exhaustive()
    }
}

impl Pool {
    /// Connects with default options. One connection is opened right away, so a bad URL or
    /// password fails here rather than on first use.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] for a bad URL, or whatever opening the first connection fails with.
    pub async fn connect(url: &str) -> Result<Self> {
        Self::connect_with(Config::from_url(url)?, PoolOptions::default()).await
    }

    /// Connects with explicit configuration.
    ///
    /// # Errors
    ///
    /// Whatever opening the first connection fails with.
    pub async fn connect_with(config: Config, options: PoolOptions) -> Result<Self> {
        let pool = Self::lazy(config, options);
        let first = pool.get().await?;
        drop(first);
        Ok(pool)
    }

    /// A pool that opens its connections on demand, without connecting now.
    #[must_use]
    pub fn lazy(config: Config, options: PoolOptions) -> Self {
        let permits = Arc::new(Semaphore::new(options.max_size.max(1)));
        Self {
            inner: Arc::new(Inner {
                config,
                options,
                idle: Mutex::new(VecDeque::new()),
                permits,
            }),
        }
    }

    /// The number of connections sitting idle.
    #[must_use]
    pub fn idle_count(&self) -> usize {
        self.inner
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// The number of connections that may still be checked out.
    #[must_use]
    pub fn available(&self) -> usize {
        self.inner.permits.available_permits()
    }

    /// Checks out a connection: an idle one if it is still healthy, a new one otherwise.
    ///
    /// # Errors
    ///
    /// [`Error::Timeout`] when none frees up in time, or the error from opening a new connection.
    pub async fn get(&self) -> Result<PooledConnection> {
        let permit = timeout(
            self.inner.options.acquire_timeout,
            self.inner.permits.clone().acquire_owned(),
        )
        .await
        .map_err(|_| Error::Timeout("waiting for a free connection"))?
        .map_err(|_| Error::Closed)?;
        loop {
            let candidate = self
                .inner
                .idle
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop_front();
            let Some(mut conn) = candidate else { break };
            if conn.looks_dead() || conn.last_used().elapsed() > self.inner.options.idle_timeout {
                continue;
            }
            if conn.last_used().elapsed() > self.inner.options.test_after_idle
                && conn.ping().await.is_err()
            {
                continue;
            }
            return Ok(PooledConnection {
                conn: Some(conn),
                pool: self.clone(),
                _permit: permit,
            });
        }
        let conn = Connection::connect(&self.inner.config).await?;
        Ok(PooledConnection {
            conn: Some(conn),
            pool: self.clone(),
            _permit: permit,
        })
    }

    fn give_back(&self, conn: Connection) {
        let options = &self.inner.options;
        if conn.is_closed()
            || conn.transaction_status() != b'I'
            || conn.created_at().elapsed() > options.max_lifetime
        {
            return;
        }
        self.inner
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(conn);
    }

    /// Runs a parameterised query on a pooled connection.
    ///
    /// # Errors
    ///
    /// See [`Connection::query`] and [`Pool::get`].
    pub async fn query(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>> {
        self.get().await?.query(sql, params).await
    }

    /// See [`Connection::query_one`].
    ///
    /// # Errors
    ///
    /// See [`Connection::query_one`] and [`Pool::get`].
    pub async fn query_one(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Row> {
        self.get().await?.query_one(sql, params).await
    }

    /// See [`Connection::query_opt`].
    ///
    /// # Errors
    ///
    /// See [`Connection::query_opt`] and [`Pool::get`].
    pub async fn query_opt(&self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>> {
        self.get().await?.query_opt(sql, params).await
    }

    /// See [`Connection::execute`].
    ///
    /// # Errors
    ///
    /// See [`Connection::execute`] and [`Pool::get`].
    pub async fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<u64> {
        self.get().await?.execute(sql, params).await
    }

    /// See [`Connection::batch_execute`].
    ///
    /// # Errors
    ///
    /// See [`Connection::batch_execute`] and [`Pool::get`].
    pub async fn batch_execute(&self, sql: &str) -> Result<()> {
        self.get().await?.batch_execute(sql).await
    }

    /// See [`Connection::simple_query`].
    ///
    /// # Errors
    ///
    /// See [`Connection::simple_query`] and [`Pool::get`].
    pub async fn simple_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        self.get().await?.simple_query(sql).await
    }

    /// Closes every idle connection. Checked-out ones are closed as they come back if the pool is dropped.
    pub async fn close(&self) {
        let drained: Vec<Connection> = self
            .inner
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect();
        for conn in drained {
            conn.close().await;
        }
    }
}

/// A connection checked out of a [`Pool`]; returned to it when dropped.
///
/// One that is closed, abandoned mid-statement, or still inside a transaction is discarded instead.
pub struct PooledConnection {
    conn: Option<Connection>,
    pool: Pool,
    _permit: OwnedSemaphorePermit,
}

impl std::fmt::Debug for PooledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledConnection")
            .field("conn", &self.conn)
            .finish_non_exhaustive()
    }
}

impl PooledConnection {
    /// Starts a transaction on this connection.
    ///
    /// # Errors
    ///
    /// See [`Connection::begin`].
    pub async fn begin(&mut self) -> Result<Transaction<'_>> {
        self.deref_mut().begin().await
    }
}

impl Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn
            .as_ref()
            .expect("a pooled connection is present until it is dropped")
    }
}

impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn
            .as_mut()
            .expect("a pooled connection is present until it is dropped")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.give_back(conn);
        }
    }
}

/// The time since `instant`, for pool statistics.
#[allow(dead_code)]
fn age(instant: Instant) -> Duration {
    instant.elapsed()
}
