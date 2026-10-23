//! One connection: transport, authentication, and request/reply.

use crate::cmd::{Cmd, cmd};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::resp::{self, Value};
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// A byte stream a connection runs over.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// A connection to one Redis server.
pub struct Connection {
    stream: Box<dyn Stream>,
    buf: Vec<u8>,
    pos: usize,
    /// A command was sent and its reply not fully read: the stream is out of step.
    busy: bool,
    closed: bool,
    command_timeout: Option<std::time::Duration>,
    created_at: Instant,
    last_used: Instant,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("closed", &self.closed)
            .field("busy", &self.busy)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "tls")]
async fn secure(config: &Config, tcp: TcpStream) -> Result<Box<dyn Stream>> {
    let options = quench_tls::ClientOptions {
        accept_invalid_certs: config.tls_insecure,
        use_system_roots: true,
        ..quench_tls::ClientOptions::default()
    };
    let tls_config = quench_tls::client_config(&options).map_err(Error::Tls)?;
    let name = quench_tls::server_name(&config.host).map_err(Error::Tls)?;
    Ok(Box::new(
        quench_tls::TlsConnector::new(tls_config)
            .connect(name, tcp)
            .await
            .map_err(|e| Error::Tls(e.to_string()))?,
    ))
}

#[cfg(not(feature = "tls"))]
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)] // keeps the signature of the `tls` build
async fn secure(_config: &Config, _tcp: TcpStream) -> Result<Box<dyn Stream>> {
    Err(Error::Tls(
        "quench-redis was built without its `tls` feature".to_string(),
    ))
}

impl Connection {
    /// Connects, negotiates TLS if asked, then authenticates and selects the database.
    ///
    /// # Errors
    ///
    /// Transport, TLS, or server errors (a wrong password is `Error::Server` of kind `WRONGPASS`).
    pub async fn connect(config: &Config) -> Result<Self> {
        timeout(config.connect_timeout, Self::connect_inner(config))
            .await
            .map_err(|_| Error::Timeout("connecting"))?
    }

    async fn connect_inner(config: &Config) -> Result<Self> {
        let tcp = TcpStream::connect((config.host.as_str(), config.port))
            .await
            .map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("connecting to {}: {e}", config.address()),
                ))
            })?;
        let _ = tcp.set_nodelay(true);
        let stream: Box<dyn Stream> = if config.tls {
            secure(config, tcp).await?
        } else {
            Box::new(tcp)
        };
        let mut conn = Self::from_stream(stream, config);
        conn.handshake(config).await?;
        Ok(conn)
    }

    /// Wraps an open stream without authenticating (tests use in-memory pipes).
    #[must_use]
    pub fn from_stream(stream: Box<dyn Stream>, config: &Config) -> Self {
        let now = Instant::now();
        Self {
            stream,
            buf: Vec::new(),
            pos: 0,
            busy: false,
            closed: false,
            command_timeout: config.command_timeout,
            created_at: now,
            last_used: now,
        }
    }

    /// `AUTH` and `SELECT`, as the configuration asks.
    ///
    /// # Errors
    ///
    /// The server's refusal, or a transport error.
    pub async fn handshake(&mut self, config: &Config) -> Result<()> {
        if let Some(password) = &config.password {
            let auth = config.username.as_ref().map_or_else(
                || cmd("AUTH").arg(password),
                |user| cmd("AUTH").arg(user).arg(password),
            );
            self.request(&auth).await?;
        }
        if config.db != 0 {
            self.request(&cmd("SELECT").arg(&config.db)).await?;
        }
        Ok(())
    }

    /// Whether the connection can no longer be used.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed || self.busy
    }

    #[must_use]
    pub const fn created_at(&self) -> Instant {
        self.created_at
    }

    #[must_use]
    pub const fn last_used(&self) -> Instant {
        self.last_used
    }

    /// Whether the peer closed an idle connection or sent stray bytes.
    pub fn looks_dead(&mut self) -> bool {
        use std::task::{Context, Poll, Waker};
        if self.is_closed() {
            return true;
        }
        let mut cx = Context::from_waker(Waker::noop());
        let mut chunk = [0u8; 1];
        let mut read_buf = tokio::io::ReadBuf::new(&mut chunk);
        !matches!(
            std::pin::Pin::new(&mut self.stream).poll_read(&mut cx, &mut read_buf),
            Poll::Pending
        )
    }

    async fn read_value(&mut self) -> Result<Value> {
        loop {
            if let Some((value, used)) =
                resp::decode(&self.buf[self.pos..]).inspect_err(|_| self.closed = true)?
            {
                self.pos += used;
                if self.pos == self.buf.len() {
                    self.buf.clear();
                    self.pos = 0;
                }
                return Ok(value);
            }
            if self.pos > 64 * 1024 {
                self.buf.drain(..self.pos);
                self.pos = 0;
            }
            self.buf.reserve(16 * 1024);
            let n = self
                .stream
                .read_buf(&mut self.buf)
                .await
                .inspect_err(|_| self.closed = true)?;
            if n == 0 {
                self.closed = true;
                return Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()));
            }
        }
    }

    async fn exchange(&mut self, request: &[u8], replies: usize) -> Result<Vec<Value>> {
        self.busy = true;
        if let Err(err) = async {
            self.stream.write_all(request).await?;
            self.stream.flush().await
        }
        .await
        {
            self.closed = true;
            return Err(err.into());
        }
        let mut values = Vec::with_capacity(replies);
        for _ in 0..replies {
            values.push(self.read_value().await?);
        }
        self.busy = false;
        self.last_used = Instant::now();
        Ok(values)
    }

    async fn limited(&mut self, request: &[u8], replies: usize) -> Result<Vec<Value>> {
        if self.closed || self.busy {
            return Err(Error::Closed);
        }
        let limit = self.command_timeout;
        let result = match limit {
            Some(limit) => timeout(limit, self.exchange(request, replies))
                .await
                .unwrap_or(Err(Error::Timeout("waiting for the reply"))),
            None => self.exchange(request, replies).await,
        };
        if matches!(result, Err(Error::Timeout(_))) {
            self.closed = true;
        }
        result
    }

    /// Sends one command and returns its reply, including an error reply as `Value::Error`.
    ///
    /// # Errors
    ///
    /// Transport and protocol errors only.
    pub async fn request_value(&mut self, command: &Cmd) -> Result<Value> {
        let mut out = Vec::new();
        command.encode(&mut out);
        Ok(self.limited(&out, 1).await?.remove(0))
    }

    /// Sends one command; a server error reply is `Err(Error::Server)`.
    ///
    /// # Errors
    ///
    /// Transport errors, or the server's error reply.
    pub async fn request(&mut self, command: &Cmd) -> Result<Value> {
        match self.request_value(command).await? {
            Value::Error(err) => Err(Error::Server(err)),
            value => Ok(value),
        }
    }

    /// Sends several commands in one write and reads all the replies; error replies stay in the list.
    ///
    /// # Errors
    ///
    /// Transport and protocol errors.
    pub async fn pipeline(&mut self, commands: &[Cmd]) -> Result<Vec<Value>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for command in commands {
            command.encode(&mut out);
        }
        self.limited(&out, commands.len()).await
    }

    /// `PING`.
    ///
    /// # Errors
    ///
    /// As [`request`](Self::request).
    pub async fn ping(&mut self) -> Result<()> {
        self.request(&cmd("PING")).await.map(|_| ())
    }

    /// Closes the stream.
    pub async fn close(mut self) {
        let _ = self.stream.shutdown().await;
    }
}
