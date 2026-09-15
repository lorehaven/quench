use bytes::Bytes;
use futures_util::Stream;
use http::{HeaderName, HeaderValue, StatusCode};
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Body as HttpBody, Frame};
use std::pin::Pin;
use std::task::{Context, Poll};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A boxed, type-erased response body. Its error type isn't `Infallible`:
/// a fixed body (`from_bytes`/`json`/...) never fails, but a
/// [`Response::streaming`] one can genuinely error mid-stream (the
/// producer side closes, a broadcast channel lags), and both need to fit
/// through the same box.
pub struct BoxBody(Pin<Box<dyn HttpBody<Data = Bytes, Error = BoxError> + Send + Sync>>);

impl BoxBody {
    pub fn new<B>(body: B) -> Self
    where
        B: HttpBody<Data = Bytes, Error = BoxError> + Send + Sync + 'static,
    {
        Self(Box::pin(body))
    }

    pub fn empty() -> Self {
        Self::new(Full::new(Bytes::new()).map_err(|never: std::convert::Infallible| match never {}))
    }
}

impl HttpBody for BoxBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.get_mut().0.as_mut().poll_frame(cx)
    }
}

/// An outgoing HTTP response.
pub struct Response {
    pub(crate) inner: http::Response<BoxBody>,
}

impl Response {
    pub fn new(status: StatusCode) -> Self {
        let inner = http::Response::builder()
            .status(status)
            .body(BoxBody::empty())
            .expect("status is always a valid response head");
        Self { inner }
    }

    pub fn status(&self) -> StatusCode {
        self.inner.status()
    }

    /// Sets a response header. Takes `&str` rather than requiring an
    /// already-lowercase `&'static str` (`HeaderName::from_static`'s
    /// requirement, which panics on a name like `"Location"` written the
    /// conventional way) - a bad name or value is dropped rather than
    /// panicking, since header names are case-insensitive on the wire
    /// regardless of how a caller happened to capitalize them.
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_ref().as_bytes()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            self.inner.headers_mut().insert(name, value);
        }
        self
    }

    /// Adds a header without replacing an existing one of the same name -
    /// what repeated `Set-Cookie` headers need (HTTP forbids folding those
    /// into one comma-joined value the way most headers can be combined).
    pub fn append_header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_ref().as_bytes()),
            HeaderValue::from_str(value.as_ref()),
        ) {
            self.inner.headers_mut().append(name, value);
        }
        self
    }

    pub fn text(status: StatusCode, body: impl Into<String>) -> Self {
        let body = body.into();
        let mut resp = Self::from_bytes(status, Bytes::from(body));
        resp = resp.header("content-type", "text/plain; charset=utf-8");
        resp
    }

    pub fn html(status: StatusCode, body: impl Into<String>) -> Self {
        Self::from_bytes(status, Bytes::from(body.into()))
            .header("content-type", "text/html; charset=utf-8")
    }

    pub fn json<T: serde::Serialize>(
        status: StatusCode,
        value: &T,
    ) -> Result<Self, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(Self::from_bytes(status, Bytes::from(bytes)).header("content-type", "application/json"))
    }

    pub fn from_bytes(status: StatusCode, bytes: Bytes) -> Self {
        let inner = http::Response::builder()
            .status(status)
            .body(BoxBody::new(
                Full::new(bytes).map_err(|never: std::convert::Infallible| match never {}),
            ))
            .expect("status is always a valid response head");
        Self { inner }
    }

    /// A chunked response body driven by `stream`: each `Ok` item becomes
    /// one chunk, an `Err` ends the stream (hyper closes the connection
    /// having already sent the headers, the same way actix-web's
    /// `.streaming()` does on a mid-stream error). What
    /// `GpuBroadcaster`/`VllmBroadcaster`-style SSE endpoints port to -
    /// format each item as `event: ...\ndata: ...\n\n` before handing it
    /// in, and set `content-type: text/event-stream` via
    /// [`header`](Self::header).
    ///
    /// This is a *response* body - unrelated to
    /// [`crate::body::InboundBody`], which streams a *request* body in.
    pub fn streaming<S, E>(status: StatusCode, stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<BoxError> + 'static,
    {
        use futures_util::StreamExt;

        let frames = stream.map(|item| item.map(Frame::data).map_err(Into::into));
        let inner = http::Response::builder()
            .status(status)
            .body(BoxBody::new(StreamBody::new(frames)))
            .expect("status is always a valid response head");
        Self { inner }
    }

    pub fn ok() -> Self {
        Self::new(StatusCode::OK)
    }

    pub fn not_found() -> Self {
        Self::text(StatusCode::NOT_FOUND, "not found")
    }

    pub fn into_hyper(self) -> http::Response<BoxBody> {
        self.inner
    }
}

impl From<()> for Response {
    fn from(_: ()) -> Self {
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
