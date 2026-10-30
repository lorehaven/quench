//! The small HTTP(S) client the rest of the workspace builds on.
//!
//! Keep-alive connection pooling, HTTP/1.1, JSON / form bodies, basic / bearer
//! auth, a per-request timeout, redirect following, and an opt-out of
//! certificate verification for development.
//!
//! Roots come from the Mozilla set plus the operating system's CA bundle (so a
//! private CA installed there works).

use quench_core::base64;
use quench_core::bytes::Bytes;
use quench_core::stream::{BoxStream, StreamExt};
use quench_proto::Body;
use quench_proto::h1::client::{ClientConfig, ClientConn, send_request};
use quench_proto::io::Io;
#[cfg(feature = "tls")]
use quench_proto::tls::{
    ClientOptions, TlsConnector, client_config, client_config_with_identity, server_name,
};

// The protocol types a request or response speaks, so callers need not depend on `quench-proto` for them.
pub use quench_proto::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
use quench_serde::{DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;

const MAX_REDIRECTS: usize = 10;

/// What a client does with a 3xx response that carries a `Location`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redirect {
    /// Hand the 3xx response to the caller.
    None,
    /// Follow up to this many hops, then fail.
    Limited(usize),
}
/// Idle connections kept per host.
const MAX_IDLE_PER_HOST: usize = 8;
/// How long an idle connection stays in the pool.
const IDLE_TTL: Duration = Duration::from_secs(60);
/// Redirect bodies are read (to reuse the connection) only up to this size.
const REDIRECT_DRAIN_LIMIT: usize = 64 * 1024;

#[derive(Debug, quench_core::Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    Request(String),
    #[error("request failed: {0}")]
    Transport(String),
    #[error("request timed out")]
    Timeout,
    #[error("could not decode response body: {0}")]
    Decode(String),
    #[error("could not build the client: {0}")]
    Build(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Cheap to clone: clones share one connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

/// Where a connection goes: scheme security, host (no brackets) and port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Origin {
    secure: bool,
    host: String,
    port: u16,
}

struct Idle {
    conn: ClientConn<Box<dyn Io>>,
    since: Instant,
}

struct Inner {
    #[cfg(feature = "tls")]
    tls: TlsConnector,
    pool: Mutex<HashMap<Origin, Vec<Idle>>>,
    timeout: Option<Duration>,
    max_response_bytes: Option<usize>,
    user_agent: Option<HeaderValue>,
    redirect: Redirect,
    config: ClientConfig,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct ClientBuilder {
    timeout: Option<Duration>,
    accept_invalid_certs: bool,
    extra_roots_pem: Vec<Vec<u8>>,
    client_identity: Option<(Vec<u8>, Vec<u8>)>,
    max_response_bytes: Option<usize>,
    user_agent: Option<String>,
    redirect: Redirect,
}

impl Client {
    #[must_use]
    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("default client configuration is valid")
    }

    #[must_use]
    pub const fn builder() -> ClientBuilder {
        ClientBuilder {
            timeout: None,
            accept_invalid_certs: false,
            extra_roots_pem: Vec::new(),
            client_identity: None,
            max_response_bytes: None,
            user_agent: None,
            redirect: Redirect::Limited(MAX_REDIRECTS),
        }
    }

    pub fn get(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn put(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn patch(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }

    pub fn delete(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn head(&self, url: impl AsRef<str>) -> RequestBuilder {
        self.request(Method::HEAD, url)
    }

    pub fn request(&self, method: Method, url: impl AsRef<str>) -> RequestBuilder {
        let text = url.as_ref();
        // A fragment is never sent, so it is dropped here.
        let without_fragment = text.split('#').next().unwrap_or(text);
        RequestBuilder {
            client: self.clone(),
            method,
            url: without_fragment
                .parse::<Uri>()
                .map_err(|e| Error::Request(format!("{text}: {e}"))),
            headers: self.default_headers(),
            body: Bytes::new(),
            timeout: self.inner.timeout,
            error: None,
        }
    }

    /// What every request starts with: the client's `User-Agent`, if it has one.
    fn default_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(agent) = &self.inner.user_agent {
            headers.insert("user-agent", agent.clone());
        }
        headers
    }

    fn take_idle(&self, origin: &Origin) -> Option<ClientConn<Box<dyn Io>>> {
        let mut pool = self
            .inner
            .pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let idle = pool.get_mut(origin)?;
        let mut found = None;
        while let Some(mut candidate) = idle.pop() {
            if candidate.since.elapsed() < IDLE_TTL && candidate.conn.is_reusable() {
                found = Some(candidate.conn);
                break;
            }
        }
        drop(pool);
        found
    }

    fn put_idle(&self, origin: Origin, conn: ClientConn<Box<dyn Io>>) {
        let mut pool = self
            .inner
            .pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let idle = pool.entry(origin).or_default();
        idle.retain(|c| c.since.elapsed() < IDLE_TTL);
        if idle.len() < MAX_IDLE_PER_HOST {
            idle.push(Idle {
                conn,
                since: Instant::now(),
            });
        }
        drop(pool);
    }

    async fn connect(&self, origin: &Origin) -> Result<ClientConn<Box<dyn Io>>> {
        let tcp = TcpStream::connect((origin.host.as_str(), origin.port))
            .await
            .map_err(|e| {
                Error::Transport(format!(
                    "connecting to {}:{}: {e}",
                    origin.host, origin.port
                ))
            })?;
        let _ = tcp.set_nodelay(true);
        let io: Box<dyn Io> = if origin.secure {
            self.secure(origin, tcp).await?
        } else {
            Box::new(tcp)
        };
        Ok(ClientConn::new(io))
    }
}

#[cfg(feature = "tls")]
impl Client {
    async fn secure(&self, origin: &Origin, tcp: TcpStream) -> Result<Box<dyn Io>> {
        let name = server_name(&origin.host).map_err(Error::Transport)?;
        let tls = self.inner.tls.connect(name, tcp).await.map_err(|e| {
            Error::Transport(format!(
                "TLS handshake with {}: {}",
                origin.host,
                error_chain(&e)
            ))
        })?;
        Ok(Box::new(tls))
    }
}

#[cfg(not(feature = "tls"))]
impl Client {
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)] // keeps the signature of the `tls` build
    async fn secure(&self, origin: &Origin, _tcp: TcpStream) -> Result<Box<dyn Io>> {
        Err(Error::Transport(format!(
            "cannot reach {} over https: quench-client was built without its `tls` feature",
            origin.host
        )))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl ClientBuilder {
    /// Applies to every request from the built client unless overridden.
    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Skips certificate verification entirely - development only.
    #[must_use]
    pub const fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.accept_invalid_certs = accept;
        self
    }

    /// Trusts an additional CA certificate (PEM) on top of the built-in and system roots.
    #[must_use]
    pub fn add_root_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.extra_roots_pem.push(pem.into());
        self
    }

    /// Presents this certificate chain and private key (both PEM) to servers that ask for a client certificate
    /// (mutual TLS). An unusable pair makes [`build`](Self::build) fail.
    #[must_use]
    pub fn client_identity_pem(
        mut self,
        certificate: impl Into<Vec<u8>>,
        private_key: impl Into<Vec<u8>>,
    ) -> Self {
        self.client_identity = Some((certificate.into(), private_key.into()));
        self
    }

    /// Sends `value` as the `User-Agent` of every request (some APIs, GitHub's among them, refuse a request
    /// without one). A request can still set its own. A value that is not a valid header makes
    /// [`build`](Self::build) fail.
    #[must_use]
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = Some(value.into());
        self
    }

    /// How redirects are handled (follows up to 10 by default). With [`Redirect::None`] a 3xx response is returned
    /// like any other, `Location` header included.
    #[must_use]
    pub const fn redirect(mut self, policy: Redirect) -> Self {
        self.redirect = policy;
        self
    }

    /// Fails a request whose response body exceeds `limit` bytes (unlimited by default).
    #[must_use]
    pub const fn max_response_bytes(mut self, limit: usize) -> Self {
        self.max_response_bytes = Some(limit);
        self
    }

    pub fn build(self) -> Result<Client> {
        #[cfg(feature = "tls")]
        let tls = {
            let options = ClientOptions {
                accept_invalid_certs: self.accept_invalid_certs,
                extra_roots_pem: self.extra_roots_pem,
                use_system_roots: true,
                alpn_protocols: vec![b"http/1.1".to_vec()],
            };
            let config = match &self.client_identity {
                Some((certificate, key)) => client_config_with_identity(&options, certificate, key),
                None => client_config(&options),
            };
            TlsConnector::new(config.map_err(Error::Build)?)
        };
        let user_agent = self
            .user_agent
            .map(|agent| {
                HeaderValue::from_str(&agent)
                    .map_err(|_| Error::Build(format!("`{agent}` is not a valid User-Agent")))
            })
            .transpose()?;
        Ok(Client {
            inner: Arc::new(Inner {
                #[cfg(feature = "tls")]
                tls,
                user_agent,
                redirect: self.redirect,
                pool: Mutex::new(HashMap::new()),
                timeout: self.timeout,
                max_response_bytes: self.max_response_bytes,
                config: ClientConfig::default(),
            }),
        })
    }
}

pub struct RequestBuilder {
    client: Client,
    method: Method,
    url: Result<Uri>,
    headers: HeaderMap,
    body: Bytes,
    timeout: Option<Duration>,
    error: Option<Error>,
}

// Hand-written: the headers can carry `Authorization`, so only their names are shown.
impl std::fmt::Debug for RequestBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestBuilder")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl RequestBuilder {
    #[must_use]
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        match (
            quench_proto::HeaderName::from_bytes(name.as_ref().as_bytes()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            (Ok(name), Ok(value)) => {
                // The client-wide `User-Agent` is a default: a request's own replaces it.
                if name.as_str() == "user-agent" {
                    self.headers.insert(name, value);
                } else {
                    self.headers.append(name, value);
                }
            }
            _ => {
                self.error.get_or_insert_with(|| {
                    Error::Request(format!("invalid header `{}`", name.as_ref()))
                });
            }
        }
        self
    }

    /// Appends `pairs` to the URL's query string, percent-encoded (`?a=1&b=x%20y`; `&` when it already has one).
    #[must_use]
    pub fn query<K: AsRef<str>, V: AsRef<str>>(mut self, pairs: &[(K, V)]) -> Self {
        if pairs.is_empty() {
            return self;
        }
        if let Ok(url) = &self.url {
            let encoded: Vec<String> = pairs
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{}={}",
                        quench_serde::urlencoded::percent_encode(key.as_ref()),
                        quench_serde::urlencoded::percent_encode(value.as_ref())
                    )
                })
                .collect();
            let text = url.as_str();
            let separator = if text.contains('?') { '&' } else { '?' };
            self.url = Uri::parse(&format!("{text}{separator}{}", encoded.join("&")))
                .map_err(|e| Error::Request(format!("{text}: {e}")));
        }
        self
    }

    /// Adds every header of `headers` (several values of one name are all sent).
    #[must_use]
    #[allow(clippy::needless_pass_by_value)] // by value, like the other clients' `headers`, so call sites need no `&`
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        for (name, value) in headers.iter() {
            self.headers.append(name.clone(), value.clone());
        }
        self
    }

    #[must_use]
    pub fn basic_auth(self, username: impl AsRef<str>, password: Option<&str>) -> Self {
        let credentials = format!("{}:{}", username.as_ref(), password.unwrap_or(""));
        self.header(
            "authorization",
            format!("Basic {}", base64::STANDARD.encode(credentials)),
        )
    }

    #[must_use]
    pub fn bearer_auth(self, token: impl AsRef<str>) -> Self {
        self.header("authorization", format!("Bearer {}", token.as_ref()))
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn json<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.body = quench_serde::json::to_vec(body).into();
        self.header("content-type", "application/json")
    }

    #[must_use]
    pub fn form<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        match quench_serde::urlencoded::to_string(body) {
            Ok(encoded) => {
                self.body = encoded.into();
                self.header("content-type", "application/x-www-form-urlencoded")
            }
            Err(e) => {
                self.error
                    .get_or_insert_with(|| Error::Request(e.to_string()));
                self
            }
        }
    }

    /// A `multipart/form-data` body; sets the `Content-Type` with its boundary.
    #[must_use]
    pub fn multipart(mut self, form: crate::multipart::Form) -> Self {
        let content_type = form.content_type();
        self.body = form.into_body();
        self.header("content-type", content_type)
    }

    /// Sends `cookies` in one `Cookie` header (`a=1; b=2`).
    #[must_use]
    pub fn cookies<K: AsRef<str>, V: AsRef<str>>(self, cookies: &[(K, V)]) -> Self {
        if cookies.is_empty() {
            return self;
        }
        let line = cookies
            .iter()
            .map(|(name, value)| format!("{}={}", name.as_ref(), value.as_ref()))
            .collect::<Vec<_>>()
            .join("; ");
        self.header("cookie", line)
    }

    /// A raw request body; set `Content-Type` yourself.
    #[must_use]
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }

    /// Like [`send`](Self::send) but returns as soon as the status and headers are in, leaving the body to be
    /// read with [`Response::bytes_stream`] (or `bytes` / `text` / `json`). The timeout covers getting the
    /// headers, not the body, and the connection is not reused afterwards.
    pub async fn send_streaming(self) -> Result<Response> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let url = self.url?;
        let exchange = exchange(
            &self.client,
            self.method,
            url,
            self.headers,
            self.body,
            true,
        );
        match self.timeout {
            Some(limit) => tokio::time::timeout(limit, exchange)
                .await
                .map_err(|_| Error::Timeout)?,
            None => exchange.await,
        }
    }

    pub async fn send(self) -> Result<Response> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let url = self.url?;
        let exchange = exchange(
            &self.client,
            self.method,
            url,
            self.headers,
            self.body,
            false,
        );
        match self.timeout {
            Some(limit) => tokio::time::timeout(limit, exchange)
                .await
                .map_err(|_| Error::Timeout)?,
            None => exchange.await,
        }
    }
}

fn origin_of(url: &Uri) -> Result<Origin> {
    let secure = match url.scheme() {
        Some(s) if s.eq_ignore_ascii_case("https") => true,
        Some(s) if s.eq_ignore_ascii_case("http") => false,
        Some(other) => return Err(Error::Request(format!("unsupported URL scheme `{other}`"))),
        None => return Err(Error::Request(format!("`{url}` is not an absolute URL"))),
    };
    let host = url
        .host()
        .ok_or_else(|| Error::Request(format!("`{url}` is not an absolute URL")))?;
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let port = url.port_u16().unwrap_or(if secure { 443 } else { 80 });
    Ok(Origin { secure, host, port })
}

/// One request/response on a pooled or fresh connection; retries once on a
/// fresh connection when a reused one turned out to be dead.
async fn round_trip(
    client: &Client,
    origin: &Origin,
    method: &Method,
    url: &Uri,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<quench_proto::h1::client::Exchange<Box<dyn Io>>> {
    for attempt in 0..2 {
        let (conn, reused) = match client.take_idle(origin) {
            Some(conn) => (conn, true),
            None => (client.connect(origin).await?, false),
        };
        let mut request = quench_proto::Request::new(method.clone(), url.clone());
        request.headers = headers.clone();
        request.body = Body::from_bytes(body.clone());
        match send_request(conn, request, &client.inner.config).await {
            Ok(exchange) => return Ok(exchange),
            Err(err)
                if reused && attempt == 0 && method.is_idempotent() && is_dead_connection(&err) => {
            }
            Err(err) => return Err(Error::Transport(error_chain(&err))),
        }
    }
    Err(Error::Transport("connection failed".to_string()))
}

const fn is_dead_connection(err: &quench_proto::Error) -> bool {
    matches!(
        err,
        quench_proto::Error::Io(_) | quench_proto::Error::UnexpectedEof
    )
}

async fn exchange(
    client: &Client,
    mut method: Method,
    mut url: Uri,
    headers: HeaderMap,
    mut body: Bytes,
    stream: bool,
) -> Result<Response> {
    let mut headers = headers;
    let (follow, hops) = match client.inner.redirect {
        Redirect::None => (false, 0),
        Redirect::Limited(limit) => (true, limit),
    };
    for _ in 0..=hops {
        let origin = origin_of(&url)?;
        let result = round_trip(client, &origin, &method, &url, &headers, &body).await?;
        let (status, response_headers) = (result.response.status, result.response.headers.clone());
        let redirect = follow
            && status.is_redirection()
            && status != StatusCode::NOT_MODIFIED
            && response_headers.contains_key("location");

        if redirect {
            // Read what we can so the connection goes back to the pool; a big body just costs the connection.
            if let Ok(_drained) = result
                .response
                .body
                .collect_limited(REDIRECT_DRAIN_LIMIT)
                .await
                && let Ok(conn) = result.reuse.await
            {
                client.put_idle(origin.clone(), conn);
            }
            let location = response_headers
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    Error::Transport("redirect with an unreadable Location".to_string())
                })?;
            let next = url
                .resolve(location)
                .map_err(|e| Error::Transport(format!("bad redirect target `{location}`: {e}")))?;
            // Credentials belong to the origin they were given for; never carry them to another one.
            if origin_of(&next)? != origin {
                for name in ["authorization", "cookie", "proxy-authorization"] {
                    headers.remove(name);
                }
            }
            url = next;
            // 301/302/303 turn a write into a GET; 307/308 replay it as is.
            if matches!(
                status,
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
            ) && method != Method::GET
                && method != Method::HEAD
            {
                method = Method::GET;
                body = Bytes::new();
                headers.remove("content-type");
            }
            continue;
        }

        if stream {
            // The caller reads the body as it arrives; the connection is not pooled afterwards.
            return Ok(Response {
                status,
                headers: response_headers,
                body: ResponseBody::Streaming(std::panic::AssertUnwindSafe(result.response.body)),
                limit: client.inner.max_response_bytes.unwrap_or(usize::MAX),
            });
        }

        let limit = client.inner.max_response_bytes.unwrap_or(usize::MAX);
        let bytes = result
            .response
            .body
            .collect_limited(limit)
            .await
            .map_err(|e| Error::Transport(e.to_string()))?;
        if let Ok(conn) = result.reuse.await {
            client.put_idle(origin, conn);
        }
        return Ok(Response {
            status,
            headers: response_headers,
            body: ResponseBody::Buffered(bytes),
            limit,
        });
    }
    Err(Error::Transport("too many redirects".to_string()))
}

/// `(name, value)` for each `Set-Cookie` header.
fn set_cookies(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .get_all("set-cookie")
        .filter_map(|value| value.to_str().ok())
        .filter_map(|line| line.split(';').next())
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(next) = source {
        out.push_str(": ");
        out.push_str(&next.to_string());
        source = next.source();
    }
    out
}

#[derive(Debug)]
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: ResponseBody,
    /// The client's `max_response_bytes`, applied when a streamed body is collected after all.
    limit: usize,
}

#[derive(Debug)]
enum ResponseBody {
    Buffered(Bytes),
    // `AssertUnwindSafe`: a response held across a `catch_unwind` stays usable, as it was before streaming.
    Streaming(std::panic::AssertUnwindSafe<Body>),
}

// The body is already buffered, so these never await - async keeps the signatures
// stable if streaming is added later.
#[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
impl Response {
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The `name=value` pairs of the response's `Set-Cookie` headers (attributes such as `Path` are dropped).
    #[must_use]
    pub fn cookies(&self) -> Vec<(String, String)> {
        set_cookies(&self.headers)
    }

    /// The value a `Set-Cookie` header gives `name`.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.cookies()
            .into_iter()
            .find_map(|(n, value)| (n == name).then_some(value))
    }

    /// The whole body. For a [`send_streaming`](RequestBuilder::send_streaming) response this reads the rest
    /// of it (subject to the client's `max_response_bytes`).
    pub async fn bytes(self) -> Result<Bytes> {
        match self.body {
            ResponseBody::Buffered(bytes) => Ok(bytes),
            ResponseBody::Streaming(body) => body
                .0
                .collect_limited(self.limit)
                .await
                .map_err(|e| Error::Transport(e.to_string())),
        }
    }

    pub async fn text(self) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.bytes().await?).into_owned())
    }

    pub async fn json<T: DeserializeOwned>(self) -> Result<T> {
        quench_serde::json::from_slice(&self.bytes().await?)
            .map_err(|e| Error::Decode(e.to_string()))
    }

    /// The body as it arrives, for responses that are long-lived or too big to hold (server-sent events, a
    /// model's token stream). A buffered response is one chunk.
    #[must_use]
    pub fn bytes_stream(self) -> BoxStream<'static, Result<Bytes>> {
        match self.body {
            ResponseBody::Buffered(bytes) => Box::pin(quench_core::stream::once(Ok(bytes))),
            ResponseBody::Streaming(body) => Box::pin(StreamExt::map(body.0, |chunk| {
                chunk.map_err(|e| Error::Transport(e.to_string()))
            })),
        }
    }
}
