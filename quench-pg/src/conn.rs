//! One connection: transport, startup and authentication, and the simple and
//! extended query protocols.

use crate::config::{Config, SslMode};
use crate::error::{Error, Result, ServerError};
use crate::message::{self, Backend};
use crate::row::Row;
use crate::scram::{self, Scram};
use crate::types::{Column, ToSql};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// A byte stream a connection runs over: TCP, a Unix socket, TLS, an in-memory pipe in tests.
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}

/// The rows and command tag of one statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryResult {
    pub rows: Vec<Row>,
    /// The command tag, e.g. `SELECT 3` or `INSERT 0 1`.
    pub tag: String,
}

impl QueryResult {
    /// The number of rows the command affected or returned.
    #[must_use]
    pub fn rows_affected(&self) -> u64 {
        message::rows_affected(&self.tag)
    }
}

/// A connection to a PostgreSQL server.
pub struct Connection {
    stream: Box<dyn Stream>,
    buf: Vec<u8>,
    pos: usize,
    params: HashMap<String, String>,
    process_id: u32,
    secret_key: u32,
    /// The transaction status from the last `ReadyForQuery`: `I`, `T` or `E`.
    status: u8,
    /// An operation started and has not seen its `ReadyForQuery`: the connection is mid-conversation.
    busy: bool,
    closed: bool,
    /// A transaction guard was dropped without finishing; roll back before the next statement.
    needs_rollback: bool,
    config: Arc<Config>,
    created_at: Instant,
    last_used: Instant,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("process_id", &self.process_id)
            .field("status", &(self.status as char))
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Connects, negotiates TLS if configured, and authenticates.
    ///
    /// # Errors
    ///
    /// Any [`Error`]: configuration, transport, TLS, authentication or a server rejection.
    pub async fn connect(config: &Config) -> Result<Self> {
        let config = Arc::new(config.clone());
        timeout(config.connect_timeout, Self::connect_inner(config.clone()))
            .await
            .map_err(|_| Error::Timeout("connecting"))?
    }

    async fn connect_inner(config: Arc<Config>) -> Result<Self> {
        let (stream, is_unix) = open_transport(&config).await?;
        let stream = if is_unix || config.ssl_mode == SslMode::Disable {
            stream
        } else {
            negotiate_tls(stream, &config).await?
        };
        let mut conn = Self::from_stream(stream, config);
        conn.startup().await?;
        Ok(conn)
    }

    /// Wraps an already-open stream; call [`startup`](Self::startup) next. Tests use this with in-memory pipes.
    #[must_use]
    pub fn from_stream(stream: Box<dyn Stream>, config: Arc<Config>) -> Self {
        let now = Instant::now();
        Self {
            stream,
            buf: Vec::new(),
            pos: 0,
            params: HashMap::new(),
            process_id: 0,
            secret_key: 0,
            status: b'I',
            busy: false,
            closed: false,
            needs_rollback: false,
            config,
            created_at: now,
            last_used: now,
        }
    }

    // ---- reading ---------------------------------------------------------------------

    async fn fill(&mut self) -> Result<()> {
        if self.pos > 0 && (self.pos == self.buf.len() || self.pos > 64 * 1024) {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.reserve(16 * 1024);
        let n = self.stream.read_buf(&mut self.buf).await?;
        if n == 0 {
            self.closed = true;
            return Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()));
        }
        Ok(())
    }

    async fn read_raw(&mut self) -> Result<(u8, Vec<u8>)> {
        loop {
            let available = &self.buf[self.pos..];
            if available.len() >= 5 {
                let tag = available[0];
                let len =
                    u32::from_be_bytes([available[1], available[2], available[3], available[4]])
                        as usize;
                if len < 4 || len - 4 > self.config.max_message_size {
                    self.closed = true;
                    return Err(Error::Protocol(format!(
                        "message length {len} is out of range"
                    )));
                }
                if available.len() > len {
                    let body = available[5..=len].to_vec();
                    self.pos += 1 + len;
                    return Ok((tag, body));
                }
            }
            self.fill().await?;
        }
    }

    /// The next message that matters: notices, parameter changes and notifications are handled on the way.
    async fn recv(&mut self) -> Result<Backend> {
        loop {
            let (tag, body) = self.read_raw().await?;
            match message::decode(tag, &body) {
                Ok(Backend::NoticeResponse(notice)) => {
                    quench_log::debug!("postgres notice: {}", notice.message);
                }
                Ok(Backend::ParameterStatus { name, value }) => {
                    self.params.insert(name, value);
                }
                Ok(Backend::NotificationResponse { channel, .. }) => {
                    quench_log::debug!("postgres notification on `{channel}` ignored");
                }
                Ok(other) => return Ok(other),
                Err(err) => {
                    self.closed = true;
                    return Err(err);
                }
            }
        }
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        if let Err(err) = async {
            self.stream.write_all(data).await?;
            self.stream.flush().await
        }
        .await
        {
            self.closed = true;
            return Err(err.into());
        }
        Ok(())
    }

    // ---- startup ---------------------------------------------------------------------

    /// Sends the startup message and completes authentication.
    ///
    /// # Errors
    ///
    /// [`Error::Auth`] for an unsupported or failed exchange, [`Error::Server`] for a rejection.
    pub async fn startup(&mut self) -> Result<()> {
        let params = self.config.startup_params();
        let pairs: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.send(&message::startup(&pairs)).await?;

        let mut scram: Option<Scram> = None;
        loop {
            match self.recv().await? {
                Backend::AuthenticationOk | Backend::ParameterDescription(_) => {}
                Backend::AuthenticationCleartextPassword => {
                    let password = self.password()?;
                    self.send(&message::password(&password)).await?;
                }
                Backend::AuthenticationMd5Password(_) => {
                    return Err(Error::Auth("the server asked for MD5 authentication, which is not supported; use scram-sha-256".to_string()));
                }
                Backend::AuthenticationSasl(mechanisms) => {
                    if !mechanisms.iter().any(|m| m == scram::MECHANISM) {
                        return Err(Error::Auth(format!(
                            "the server offers only {mechanisms:?}; SCRAM-SHA-256 is required"
                        )));
                    }
                    let exchange = Scram::new(&self.password()?);
                    self.send(&message::sasl_initial_response(
                        scram::MECHANISM,
                        &exchange.client_first(),
                    ))
                    .await?;
                    scram = Some(exchange);
                }
                Backend::AuthenticationSaslContinue(server_first) => {
                    let exchange = scram.as_mut().ok_or_else(|| {
                        Error::Protocol("SASL continue without a SASL start".to_string())
                    })?;
                    let reply = exchange.client_final(&server_first)?;
                    self.send(&message::sasl_response(&reply)).await?;
                }
                Backend::AuthenticationSaslFinal(server_final) => {
                    scram
                        .as_ref()
                        .ok_or_else(|| {
                            Error::Protocol("SASL final without a SASL start".to_string())
                        })?
                        .verify_server(&server_final)?;
                }
                Backend::AuthenticationOther(kind) => {
                    return Err(Error::Auth(format!(
                        "unsupported authentication method {kind}"
                    )));
                }
                Backend::BackendKeyData {
                    process_id,
                    secret_key,
                } => {
                    self.process_id = process_id;
                    self.secret_key = secret_key;
                }
                Backend::ReadyForQuery(status) => {
                    self.status = status;
                    return Ok(());
                }
                Backend::ErrorResponse(err) => {
                    self.closed = true;
                    return Err(Error::Server(Box::new(err)));
                }
                other => {
                    self.closed = true;
                    return Err(Error::Protocol(format!(
                        "unexpected {other:?} during startup"
                    )));
                }
            }
        }
    }

    fn password(&self) -> Result<String> {
        self.config.password.clone().ok_or_else(|| {
            Error::Auth("the server asked for a password but none was configured".to_string())
        })
    }

    // ---- state -----------------------------------------------------------------------

    /// Whether the connection can no longer be used: closed, or abandoned in the middle of a statement.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed || self.busy
    }

    /// The server's transaction status: `b'I'` idle, `b'T'` in a transaction, `b'E'` in a failed one.
    #[must_use]
    pub const fn transaction_status(&self) -> u8 {
        self.status
    }

    /// The backend process id (for `pg_cancel_backend` and logs).
    #[must_use]
    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    /// A server parameter reported at startup or since (`server_version`, `TimeZone`, ...).
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(String::as_str)
    }

    /// When the connection was opened.
    #[must_use]
    pub const fn created_at(&self) -> Instant {
        self.created_at
    }

    /// When a statement last finished (or the connection was opened).
    #[must_use]
    pub const fn last_used(&self) -> Instant {
        self.last_used
    }

    /// Whether the peer closed an idle connection or sent stray bytes since the last statement.
    pub fn looks_dead(&mut self) -> bool {
        use std::task::{Context, Waker};
        if self.is_closed() {
            return true;
        }
        let mut cx = Context::from_waker(Waker::noop());
        let mut chunk = [0u8; 1];
        let mut read_buf = tokio::io::ReadBuf::new(&mut chunk);
        std::pin::Pin::new(&mut self.stream)
            .poll_read(&mut cx, &mut read_buf)
            .is_ready()
    }

    /// Rolls back whatever a dropped transaction guard left open, then checks the connection is usable.
    async fn prepare(&mut self) -> Result<()> {
        if self.closed || self.busy {
            return Err(Error::Closed);
        }
        if self.needs_rollback {
            self.needs_rollback = false;
            self.simple_unchecked("ROLLBACK").await?;
        }
        Ok(())
    }

    // ---- simple queries --------------------------------------------------------------

    /// Runs one or more statements with the simple query protocol (no parameters) and returns
    /// each statement's rows. Use this for scripts: several `;`-separated statements run in one call.
    ///
    /// # Errors
    ///
    /// [`Error::Server`] for the first failing statement (the rest of the script is skipped by the server).
    pub async fn simple_query(&mut self, sql: &str) -> Result<Vec<QueryResult>> {
        self.prepare().await?;
        let limit = self.config.statement_timeout;
        let result = within(limit, self.simple_unchecked(sql)).await;
        self.note_timeout(&result);
        result
    }

    /// Like [`simple_query`](Self::simple_query), ignoring the rows.
    ///
    /// # Errors
    ///
    /// As [`simple_query`](Self::simple_query).
    pub async fn batch_execute(&mut self, sql: &str) -> Result<()> {
        self.simple_query(sql).await.map(|_| ())
    }

    async fn simple_unchecked(&mut self, sql: &str) -> Result<Vec<QueryResult>> {
        self.busy = true;
        self.send(&message::query(sql)).await?;
        let mut results = Vec::new();
        let mut columns: Option<Arc<Vec<Column>>> = None;
        let mut rows = Vec::new();
        let mut failure: Option<ServerError> = None;
        loop {
            match self.recv().await? {
                Backend::RowDescription(fields) => {
                    columns = Some(Arc::new(describe_columns(fields)));
                }
                Backend::DataRow(values) => {
                    let columns = columns.clone().ok_or_else(|| {
                        Error::Protocol("a data row before its description".to_string())
                    })?;
                    rows.push(build_row(columns, values)?);
                }
                Backend::CommandComplete(tag) => {
                    columns = None;
                    results.push(QueryResult {
                        rows: std::mem::take(&mut rows),
                        tag,
                    });
                }
                Backend::EmptyQueryResponse => results.push(QueryResult {
                    rows: Vec::new(),
                    tag: String::new(),
                }),
                Backend::ErrorResponse(err) => {
                    failure.get_or_insert(err);
                }
                Backend::ReadyForQuery(status) => {
                    self.finish(status);
                    return failure.map_or(Ok(results), |err| Err(Error::Server(Box::new(err))));
                }
                _ => {}
            }
        }
    }

    // ---- extended queries ------------------------------------------------------------

    async fn extended(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<QueryResult> {
        if params.len() > i16::MAX as usize {
            return Err(Error::Config(format!(
                "{} parameters; the protocol allows {}",
                params.len(),
                i16::MAX
            )));
        }
        self.prepare().await?;
        let texts: Vec<Option<String>> = params.iter().map(super::types::ToSql::to_sql).collect();
        let mut out = message::parse("", sql, &vec![0; params.len()]);
        out.extend(message::bind("", "", &texts));
        out.extend(message::describe(b'P', ""));
        out.extend(message::execute("", 0));
        out.extend(message::sync());
        let limit = self.config.statement_timeout;
        let result = within(limit, self.extended_unchecked(out)).await;
        self.note_timeout(&result);
        result
    }

    async fn extended_unchecked(&mut self, request: Vec<u8>) -> Result<QueryResult> {
        self.busy = true;
        self.send(&request).await?;
        let mut columns: Option<Arc<Vec<Column>>> = None;
        let mut rows = Vec::new();
        let mut tag = String::new();
        let mut failure: Option<ServerError> = None;
        loop {
            match self.recv().await? {
                Backend::RowDescription(fields) => {
                    columns = Some(Arc::new(describe_columns(fields)));
                }
                Backend::DataRow(values) => {
                    let columns = columns.clone().ok_or_else(|| {
                        Error::Protocol("a data row before its description".to_string())
                    })?;
                    rows.push(build_row(columns, values)?);
                }
                Backend::CommandComplete(done) => tag = done,
                Backend::ErrorResponse(err) => {
                    failure.get_or_insert(err);
                }
                Backend::ReadyForQuery(status) => {
                    self.finish(status);
                    return failure.map_or(Ok(QueryResult { rows, tag }), |err| {
                        Err(Error::Server(Box::new(err)))
                    });
                }
                _ => {}
            }
        }
    }

    fn finish(&mut self, status: u8) {
        self.status = status;
        self.busy = false;
        self.last_used = Instant::now();
    }

    /// A statement that timed out left the conversation half-finished: the connection is unusable.
    const fn note_timeout<T>(&mut self, result: &Result<T>) {
        if matches!(result, Err(Error::Timeout(_))) {
            self.closed = true;
        }
    }

    /// Runs one parameterised statement and returns its rows.
    ///
    /// Parameters are `$1`, `$2`, ... in the SQL, typed by the server from context.
    ///
    /// # Errors
    ///
    /// [`Error::Server`] if the statement fails; transport errors otherwise.
    pub async fn query(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>> {
        Ok(self.extended(sql, params).await?.rows)
    }

    /// Like [`query`](Self::query), requiring exactly one row.
    ///
    /// # Errors
    ///
    /// [`Error::RowCount`] for none or several.
    pub async fn query_one(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Row> {
        let mut rows = self.query(sql, params).await?;
        if rows.len() == 1 {
            Ok(rows.remove(0))
        } else {
            Err(Error::RowCount {
                expected: "exactly one",
                got: rows.len(),
            })
        }
    }

    /// Like [`query`](Self::query), allowing zero or one row.
    ///
    /// # Errors
    ///
    /// [`Error::RowCount`] for several.
    pub async fn query_opt(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>> {
        let mut rows = self.query(sql, params).await?;
        match rows.len() {
            0 => Ok(None),
            1 => Ok(Some(rows.remove(0))),
            got => Err(Error::RowCount {
                expected: "at most one",
                got,
            }),
        }
    }

    /// Runs one parameterised statement and returns the number of rows it affected.
    ///
    /// # Errors
    ///
    /// As [`query`](Self::query).
    pub async fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<u64> {
        Ok(self.extended(sql, params).await?.rows_affected())
    }

    /// Runs `SELECT 1`; the cheap health check.
    ///
    /// # Errors
    ///
    /// Any error from the round trip.
    pub async fn ping(&mut self) -> Result<()> {
        self.simple_query("SELECT 1").await.map(|_| ())
    }

    // ---- transactions ----------------------------------------------------------------

    /// Starts a transaction. Dropping the guard without [`Transaction::commit`] rolls it back
    /// before the connection's next statement.
    ///
    /// # Errors
    ///
    /// As [`batch_execute`](Self::batch_execute).
    pub async fn begin(&mut self) -> Result<Transaction<'_>> {
        self.batch_execute("BEGIN").await?;
        Ok(Transaction {
            conn: self,
            finished: false,
        })
    }

    /// Says goodbye to the server and closes the stream.
    pub async fn close(mut self) {
        if !self.closed {
            let _ = self.send(&message::terminate()).await;
            let _ = self.stream.shutdown().await;
        }
    }
}

/// An open transaction.
#[derive(Debug)]
pub struct Transaction<'a> {
    conn: &'a mut Connection,
    finished: bool,
}

impl Transaction<'_> {
    /// See [`Connection::query`].
    ///
    /// # Errors
    ///
    /// As [`Connection::query`].
    pub async fn query(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Row>> {
        self.conn.query(sql, params).await
    }

    /// See [`Connection::query_one`].
    ///
    /// # Errors
    ///
    /// As [`Connection::query_one`].
    pub async fn query_one(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Row> {
        self.conn.query_one(sql, params).await
    }

    /// See [`Connection::query_opt`].
    ///
    /// # Errors
    ///
    /// As [`Connection::query_opt`].
    pub async fn query_opt(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<Option<Row>> {
        self.conn.query_opt(sql, params).await
    }

    /// See [`Connection::execute`].
    ///
    /// # Errors
    ///
    /// As [`Connection::execute`].
    pub async fn execute(&mut self, sql: &str, params: &[&dyn ToSql]) -> Result<u64> {
        self.conn.execute(sql, params).await
    }

    /// See [`Connection::batch_execute`].
    ///
    /// # Errors
    ///
    /// As [`Connection::batch_execute`].
    pub async fn batch_execute(&mut self, sql: &str) -> Result<()> {
        self.conn.batch_execute(sql).await
    }

    /// See [`Connection::simple_query`].
    ///
    /// # Errors
    ///
    /// As [`Connection::simple_query`].
    pub async fn simple_query(&mut self, sql: &str) -> Result<Vec<QueryResult>> {
        self.conn.simple_query(sql).await
    }

    /// Commits.
    ///
    /// # Errors
    ///
    /// The server's error if the commit fails (the transaction is then rolled back by the server).
    pub async fn commit(mut self) -> Result<()> {
        self.finished = true;
        self.conn.batch_execute("COMMIT").await
    }

    /// Rolls back.
    ///
    /// # Errors
    ///
    /// Transport errors; a rollback itself cannot fail on the server.
    pub async fn rollback(mut self) -> Result<()> {
        self.finished = true;
        self.conn.batch_execute("ROLLBACK").await
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.conn.needs_rollback = true;
        }
    }
}

/// Runs `operation` under the statement timeout, if there is one.
async fn within<T>(
    limit: Option<std::time::Duration>,
    operation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    match limit {
        Some(limit) => timeout(limit, operation)
            .await
            .unwrap_or(Err(Error::Timeout("running the statement"))),
        None => operation.await,
    }
}

fn describe_columns(fields: Vec<message::FieldDescription>) -> Vec<Column> {
    fields
        .into_iter()
        .map(|f| Column {
            name: f.name,
            type_oid: f.type_oid,
            table_oid: f.table_oid,
            column_id: f.column_id,
            type_modifier: f.type_modifier,
        })
        .collect()
}

fn build_row(columns: Arc<Vec<Column>>, values: Vec<Option<Vec<u8>>>) -> Result<Row> {
    let mut texts = Vec::with_capacity(values.len());
    for value in values {
        texts.push(match value {
            None => None,
            Some(bytes) => {
                Some(String::from_utf8(bytes).map_err(|_| {
                    Error::Protocol("a column value is not valid UTF-8".to_string())
                })?)
            }
        });
    }
    Ok(Row::new(columns, texts))
}

async fn open_transport(config: &Config) -> Result<(Box<dyn Stream>, bool)> {
    #[cfg(unix)]
    if config.host.starts_with('/') {
        let path = format!(
            "{}/.s.PGSQL.{}",
            config.host.trim_end_matches('/'),
            config.port
        );
        let socket = tokio::net::UnixStream::connect(&path).await.map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("connecting to {path}: {e}"),
            ))
        })?;
        return Ok((Box::new(socket), true));
    }
    let tcp = TcpStream::connect((config.host.as_str(), config.port))
        .await
        .map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!("connecting to {}:{}: {e}", config.host, config.port),
            ))
        })?;
    let _ = tcp.set_nodelay(true);
    Ok((Box::new(tcp), false))
}

/// Without the `tls` feature `prefer` stays on the plain stream and the modes that insist on
/// encryption fail before anything is sent.
#[cfg(not(feature = "tls"))]
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)] // keeps the signature of the `tls` build
async fn negotiate_tls(stream: Box<dyn Stream>, config: &Config) -> Result<Box<dyn Stream>> {
    match config.ssl_mode {
        SslMode::Prefer => Ok(stream),
        _ => Err(Error::Tls(
            "quench-pg was built without its `tls` feature".to_string(),
        )),
    }
}

/// Sends an `SSLRequest` and upgrades the stream if the server agrees.
#[cfg(feature = "tls")]
async fn negotiate_tls(mut stream: Box<dyn Stream>, config: &Config) -> Result<Box<dyn Stream>> {
    stream.write_all(&message::ssl_request()).await?;
    stream.flush().await?;
    let mut answer = [0u8; 1];
    stream.read_exact(&mut answer).await?;
    match answer[0] {
        b'S' => {
            let verify = config.ssl_mode == SslMode::VerifyFull;
            let options = quench_tls::ClientOptions {
                accept_invalid_certs: !verify,
                extra_roots_pem: config.ssl_root_cert_pem.clone(),
                use_system_roots: verify,
                alpn_protocols: Vec::new(),
            };
            let tls_config = quench_tls::client_config(&options).map_err(Error::Tls)?;
            let name = quench_tls::server_name(&config.host).map_err(Error::Tls)?;
            let tls = quench_tls::TlsConnector::new(tls_config)
                .connect(name, stream)
                .await
                .map_err(|e| Error::Tls(e.to_string()))?;
            Ok(Box::new(tls))
        }
        b'N' if config.ssl_mode == SslMode::Prefer => Ok(stream),
        b'N' => Err(Error::Tls("the server does not support TLS".to_string())),
        other => Err(Error::Protocol(format!(
            "unexpected answer {other:#x} to the SSL request"
        ))),
    }
}
