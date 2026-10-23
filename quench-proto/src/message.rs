//! Request and response messages.

use crate::body::Body;
use crate::extensions::Extensions;
use crate::header::{HeaderMap, HeaderValue};
use crate::method::Method;
use crate::status::StatusCode;
use crate::uri::Uri;
use crate::version::Version;

/// An HTTP request: head plus body.
#[derive(Debug, Default)]
pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub headers: HeaderMap,
    pub extensions: Extensions,
    pub body: Body,
}

impl Request {
    #[must_use]
    pub fn new(method: Method, uri: Uri) -> Self {
        Self {
            method,
            uri,
            ..Self::default()
        }
    }

    /// Sets a header, replacing earlier values. Invalid names or values are dropped.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        if let Ok(value) = HeaderValue::from_str(value) {
            self.headers.insert(name, value);
        }
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }
}

/// An HTTP response: head plus body, and optionally an upgrade to take over the connection.
#[derive(Debug, Default)]
pub struct Response {
    pub status: StatusCode,
    pub version: Version,
    pub headers: HeaderMap,
    pub extensions: Extensions,
    pub body: Body,
    pub(crate) upgrade: Option<crate::io::OnUpgrade>,
}

impl Response {
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        if let Ok(value) = HeaderValue::from_str(value) {
            self.headers.insert(name, value);
        }
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    /// Asks the connection driver to hand the raw stream to `handler` after the
    /// head (typically `101 Switching Protocols`) is written. The body is ignored.
    #[must_use]
    pub fn on_upgrade<F, Fut>(mut self, handler: F) -> Self
    where
        F: FnOnce(crate::io::Upgraded) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.upgrade = Some(crate::io::OnUpgrade::new(handler));
        self
    }

    /// Whether [`on_upgrade`](Self::on_upgrade) was called.
    #[must_use]
    pub const fn is_upgrade(&self) -> bool {
        self.upgrade.is_some()
    }
}
