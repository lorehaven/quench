use quench_core::bytes::Bytes;
use quench_core::stream::Stream;
use quench_proto::body::{Body, BoxError};
use quench_proto::{HeaderValue, StatusCode};

/// An outgoing HTTP response.
#[derive(Debug)]
pub struct Response {
    pub(crate) inner: quench_proto::Response,
}

impl Response {
    #[must_use]
    pub fn new(status: StatusCode) -> Self {
        Self {
            inner: quench_proto::Response::new(status),
        }
    }

    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.inner.status
    }

    /// Read access to the headers.
    #[must_use]
    pub const fn headers(&self) -> &quench_proto::HeaderMap {
        &self.inner.headers
    }

    /// Sets a response header, replacing earlier values. Header names are
    /// case-insensitive, so `"Location"` is fine; a bad name or value is
    /// dropped rather than panicking.
    #[must_use]
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let Ok(value) = HeaderValue::from_str(value.as_ref()) {
            self.inner.headers.insert(name.as_ref(), value);
        }
        self
    }

    /// Adds a header without replacing an existing one of the same name -
    /// what repeated `Set-Cookie` headers need (HTTP forbids folding those
    /// into one comma-joined value the way most headers can be combined).
    #[must_use]
    pub fn append_header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let Ok(value) = HeaderValue::from_str(value.as_ref()) {
            self.inner.headers.append(name.as_ref(), value);
        }
        self
    }

    /// Adds a `Set-Cookie` header for `cookie`.
    #[must_use]
    pub fn cookie(self, cookie: &quench_proto::cookie::Cookie) -> Self {
        self.append_header("set-cookie", cookie.to_string())
    }

    pub fn text(status: StatusCode, body: impl Into<String>) -> Self {
        Self::from_bytes(status, Bytes::from(body.into()))
            .header("content-type", "text/plain; charset=utf-8")
    }

    pub fn html(status: StatusCode, body: impl Into<String>) -> Self {
        Self::from_bytes(status, Bytes::from(body.into()))
            .header("content-type", "text/html; charset=utf-8")
    }

    /// A JSON response. Serialising cannot fail; the `Result` stays so call
    /// sites written for a fallible serialiser keep compiling.
    #[allow(clippy::unnecessary_wraps)]
    pub fn json<T: quench_serde::Serialize>(
        status: StatusCode,
        value: &T,
    ) -> Result<Self, quench_serde::Error> {
        let bytes = quench_serde::json::to_vec(value);
        Ok(Self::from_bytes(status, Bytes::from(bytes)).header("content-type", "application/json"))
    }

    #[must_use]
    pub fn from_bytes(status: StatusCode, bytes: Bytes) -> Self {
        let mut inner = quench_proto::Response::new(status);
        inner.body = Body::from_bytes(bytes);
        Self { inner }
    }

    /// A chunked response body driven by `stream`: each `Ok` item becomes
    /// one chunk, an `Err` ends the stream (the connection is closed
    /// having already sent the headers). What
    /// `GpuBroadcaster`/`VllmBroadcaster`-style SSE endpoints port to -
    /// format each item as `event: ...\ndata: ...\n\n` before handing it
    /// in, and set `content-type: text/event-stream` via
    /// [`header`](Self::header), or use [`sse`](Self::sse).
    ///
    /// This is a *response* body - unrelated to
    /// [`crate::body::InboundBody`], which streams a *request* body in.
    pub fn streaming<S, E>(status: StatusCode, stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<BoxError> + 'static,
    {
        let mut inner = quench_proto::Response::new(status);
        inner.body = Body::from_stream(stream);
        Self { inner }
    }

    /// A server-sent-events response (`text/event-stream`) streaming `events`.
    pub fn sse<S>(events: S) -> Self
    where
        S: Stream<Item = quench_proto::sse::Event> + Unpin + Send + Sync + 'static,
    {
        Self {
            inner: quench_proto::sse::response(events),
        }
    }

    #[must_use]
    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::text(StatusCode::NOT_FOUND, "not found")
    }

    /// The protocol-level response, for the connection driver.
    #[must_use]
    pub fn into_inner(self) -> quench_proto::Response {
        self.inner
    }
}

impl From<quench_proto::Response> for Response {
    fn from(inner: quench_proto::Response) -> Self {
        Self { inner }
    }
}

impl From<()> for Response {
    fn from((): ()) -> Self {
        Self::new(StatusCode::NO_CONTENT)
    }
}

impl From<String> for Response {
    fn from(value: String) -> Self {
        Self::text(StatusCode::OK, value)
    }
}

impl From<&'static str> for Response {
    fn from(value: &'static str) -> Self {
        Self::text(StatusCode::OK, value)
    }
}
