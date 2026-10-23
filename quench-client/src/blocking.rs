//! The same client for synchronous programs: each call blocks until the response is in.
//!
//! A [`Client`] owns a small tokio runtime that drives its connections, so the pool is reused between calls
//! and nothing needs to be `async`. Like every blocking HTTP client it must not be called from inside an async
//! runtime (tokio panics when a runtime is started within one).

use crate::multipart::Form;
use crate::transport::{self, Error, HeaderMap, Method, Redirect, StatusCode};
use quench_core::bytes::Bytes;
use quench_serde::{DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};

type Result<T> = std::result::Result<T, Error>;

fn runtime() -> Result<Arc<Runtime>> {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .map(Arc::new)
        .map_err(|e| Error::Build(format!("could not start the HTTP runtime: {e}")))
}

#[derive(Clone, Debug)]
pub struct Client {
    inner: transport::Client,
    runtime: Arc<Runtime>,
}

impl Client {
    /// # Panics
    ///
    /// When the runtime cannot start (the process is out of threads or descriptors).
    #[must_use]
    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("default client configuration is valid")
    }

    #[must_use]
    pub const fn builder() -> ClientBuilder {
        ClientBuilder {
            inner: transport::Client::builder(),
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
        RequestBuilder {
            inner: self.inner.request(method, url),
            runtime: Arc::clone(&self.runtime),
        }
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct ClientBuilder {
    inner: transport::ClientBuilder,
}

impl ClientBuilder {
    /// Applies to every request unless overridden.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Skips certificate verification entirely - development only.
    #[must_use]
    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.inner = self.inner.danger_accept_invalid_certs(accept);
        self
    }

    /// Trusts an additional CA certificate (PEM).
    #[must_use]
    pub fn add_root_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.inner = self.inner.add_root_certificate_pem(pem);
        self
    }

    /// How redirects are handled; see [`Redirect`].
    #[must_use]
    pub fn redirect(mut self, policy: Redirect) -> Self {
        self.inner = self.inner.redirect(policy);
        self
    }

    /// Sent as the `User-Agent` of every request.
    #[must_use]
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.inner = self.inner.user_agent(value);
        self
    }

    pub fn build(self) -> Result<Client> {
        Ok(Client {
            inner: self.inner.build()?,
            runtime: runtime()?,
        })
    }
}

#[derive(Debug)]
pub struct RequestBuilder {
    inner: transport::RequestBuilder,
    runtime: Arc<Runtime>,
}

impl RequestBuilder {
    #[must_use]
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.inner = self.inner.header(name, value);
        self
    }

    #[must_use]
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.inner = self.inner.headers(headers);
        self
    }

    #[must_use]
    pub fn basic_auth(mut self, username: impl AsRef<str>, password: Option<&str>) -> Self {
        self.inner = self.inner.basic_auth(username, password);
        self
    }

    #[must_use]
    pub fn bearer_auth(mut self, token: impl AsRef<str>) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }

    #[must_use]
    pub fn json<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.inner = self.inner.json(body);
        self
    }

    #[must_use]
    pub fn form<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.inner = self.inner.form(body);
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// A `multipart/form-data` body; sets the `Content-Type` with its boundary.
    #[must_use]
    pub fn multipart(mut self, form: Form) -> Self {
        self.inner = self.inner.multipart(form);
        self
    }

    /// Sends `cookies` in one `Cookie` header.
    #[must_use]
    pub fn cookies<K: AsRef<str>, V: AsRef<str>>(mut self, cookies: &[(K, V)]) -> Self {
        self.inner = self.inner.cookies(cookies);
        self
    }

    /// Appends `pairs` to the URL as a query string (percent-encoded).
    #[must_use]
    pub fn query<K: AsRef<str>, V: AsRef<str>>(mut self, pairs: &[(K, V)]) -> Self {
        self.inner = self.inner.query(pairs);
        self
    }

    /// Sends the request and waits for the whole response.
    pub fn send(self) -> Result<Response> {
        let inner = self.runtime.block_on(self.inner.send())?;
        Ok(Response {
            inner,
            runtime: self.runtime,
        })
    }
}

#[derive(Debug)]
pub struct Response {
    inner: transport::Response,
    runtime: Arc<Runtime>,
}

impl Response {
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.inner.status()
    }

    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// The `name=value` pairs of the response's `Set-Cookie` headers.
    #[must_use]
    pub fn cookies(&self) -> Vec<(String, String)> {
        self.inner.cookies()
    }

    /// The value a `Set-Cookie` header gives `name`.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.inner.cookie(name)
    }

    pub fn bytes(self) -> Result<Bytes> {
        self.runtime.block_on(self.inner.bytes())
    }

    pub fn text(self) -> Result<String> {
        self.runtime.block_on(self.inner.text())
    }

    pub fn json<T: DeserializeOwned>(self) -> Result<T> {
        self.runtime.block_on(self.inner.json())
    }
}
