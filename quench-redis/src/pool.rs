//! A pool of connections to one server, with reconnect on demand.

use crate::cmd::{Cmd, FromValue};
use crate::config::Config;
use crate::conn::Connection;
use crate::error::{Error, Result};
use crate::resp::Value;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

/// Pool limits.
#[derive(Debug, Clone)]
pub struct PoolOptions {
    /// Most connections open at once (default 8).
    pub max_size: usize,
    /// How long [`Client::get`] waits for a free connection (default 10 s).
    pub acquire_timeout: Duration,
    /// Idle connections older than this are closed instead of reused (default 5 minutes).
    pub idle_timeout: Duration,
}

impl Default for PoolOptions {
    fn default() -> Self {
        Self {
            max_size: 8,
            acquire_timeout: Duration::from_secs(10),
            idle_timeout: Duration::from_secs(300),
        }
    }
}

struct Inner {
    config: Config,
    options: PoolOptions,
    idle: Mutex<VecDeque<Connection>>,
    permits: Arc<Semaphore>,
}

/// A cheap-to-clone handle on a pool of connections to one server.
///
/// A connection that died while idle is replaced transparently: the first command that finds a
/// reused connection dead is retried once on a fresh one. That makes a command that
/// failed after the server executed it run twice, so prefer idempotent commands across restarts.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("address", &self.inner.config.address())
            .field("idle", &self.idle_count())
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Connects to `url` (`redis://...`), opening one connection now so a bad URL or password fails here.
    ///
    /// # Errors
    ///
    /// [`Error::Config`] for the URL, or whatever opening the first connection fails with.
    pub async fn connect(url: &str) -> Result<Self> {
        Self::connect_with(Config::from_url(url)?, PoolOptions::default()).await
    }

    /// As [`connect`](Self::connect), with explicit configuration.
    ///
    /// # Errors
    ///
    /// Whatever opening the first connection fails with.
    pub async fn connect_with(config: Config, options: PoolOptions) -> Result<Self> {
        let client = Self::lazy(config, options);
        drop(client.get().await?);
        Ok(client)
    }

    /// A client that connects on first use.
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

    /// The server's address.
    #[must_use]
    pub fn address(&self) -> String {
        self.inner.config.address()
    }

    #[must_use]
    pub fn idle_count(&self) -> usize {
        self.inner
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Checks out a connection (an idle one if it is alive, a new one otherwise).
    ///
    /// # Errors
    ///
    /// [`Error::Timeout`] when none frees up in time, or the error from connecting.
    pub async fn get(&self) -> Result<PooledConnection> {
        self.checkout().await.map(|(conn, _)| conn)
    }

    /// Like [`get`](Self::get), also telling whether the connection was reused.
    async fn checkout(&self) -> Result<(PooledConnection, bool)> {
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
            return Ok((
                PooledConnection {
                    conn: Some(conn),
                    client: self.clone(),
                    _permit: permit,
                },
                true,
            ));
        }
        let conn = Connection::connect(&self.inner.config).await?;
        Ok((
            PooledConnection {
                conn: Some(conn),
                client: self.clone(),
                _permit: permit,
            },
            false,
        ))
    }

    fn give_back(&self, conn: Connection) {
        if !conn.is_closed() {
            self.inner
                .idle
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(conn);
        }
    }

    /// Runs a command, returning an error reply as `Value::Error` (the cluster layer reads redirects from it).
    ///
    /// # Errors
    ///
    /// Transport and protocol errors.
    pub async fn exec_value(&self, command: &Cmd) -> Result<Value> {
        for attempt in 0..2 {
            let (mut conn, reused) = self.checkout().await?;
            match conn.request_value(command).await {
                Err(Error::Io(_) | Error::Closed | Error::Protocol(_))
                    if reused && attempt == 0 => {}
                other => return other,
            }
        }
        Err(Error::Closed)
    }

    /// Runs a command and converts the reply.
    ///
    /// # Errors
    ///
    /// Transport errors, the server's error reply, or a reply that does not fit `T`.
    pub async fn exec<T: FromValue>(&self, command: &Cmd) -> Result<T> {
        match self.exec_value(command).await? {
            Value::Error(err) => Err(Error::Server(err)),
            value => T::from_value(value),
        }
    }

    /// Runs commands in one round trip; error replies stay in the list as `Value::Error`.
    ///
    /// # Errors
    ///
    /// Transport and protocol errors.
    pub async fn pipeline(&self, commands: &[Cmd]) -> Result<Vec<Value>> {
        self.get().await?.pipeline(commands).await
    }

    /// Closes every idle connection.
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

/// A connection checked out of a [`Client`]; returned when dropped, unless it is no longer usable.
pub struct PooledConnection {
    conn: Option<Connection>,
    client: Client,
    _permit: OwnedSemaphorePermit,
}

impl std::fmt::Debug for PooledConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PooledConnection")
            .field("conn", &self.conn)
            .finish_non_exhaustive()
    }
}

impl Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("present until dropped")
    }
}

impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("present until dropped")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.client.give_back(conn);
        }
    }
}
