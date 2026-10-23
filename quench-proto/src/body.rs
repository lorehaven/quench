//! Message bodies: a stream of byte chunks with an optional known length.

use quench_core::bytes::Bytes;
use quench_core::stream::Stream;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

/// The error type a body stream can fail with.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

type Chunks = Pin<Box<dyn Stream<Item = Result<Bytes, BoxError>> + Send + Sync>>;

enum Kind {
    Empty,
    Full(Option<Bytes>),
    Stream { chunks: Chunks, len: Option<u64> },
}

/// A request or response body.
///
/// Fixed bodies know their length up front, which lets the writer send a
/// `Content-Length`; streams are sent chunked unless their length was declared
/// with [`Body::from_stream_sized`].
pub struct Body(Kind);

/// Why [`Body::collect_limited`] gave up.
#[derive(Debug)]
pub enum CollectError {
    /// More than `limit` bytes arrived.
    TooLarge(usize),
    /// The stream failed.
    Read(BoxError),
}

impl fmt::Display for CollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(limit) => write!(f, "body exceeds {limit} bytes"),
            Self::Read(err) => write!(f, "failed to read body: {err}"),
        }
    }
}

impl std::error::Error for CollectError {}

impl Body {
    #[must_use]
    pub const fn empty() -> Self {
        Self(Kind::Empty)
    }

    #[must_use]
    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        let bytes = bytes.into();
        if bytes.is_empty() {
            Self::empty()
        } else {
            Self(Kind::Full(Some(bytes)))
        }
    }

    /// A body of unknown length: the writer sends it chunked.
    pub fn from_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<BoxError> + 'static,
    {
        Self(Kind::Stream {
            chunks: Box::pin(MapErr(Box::pin(stream))),
            len: None,
        })
    }

    /// A streamed body whose total length is known to be `len`.
    pub fn from_stream_sized<S, E>(stream: S, len: u64) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<BoxError> + 'static,
    {
        Self(Kind::Stream {
            chunks: Box::pin(MapErr(Box::pin(stream))),
            len: Some(len),
        })
    }

    /// The exact length, when known without reading.
    #[must_use]
    pub fn exact_len(&self) -> Option<u64> {
        match &self.0 {
            Kind::Empty => Some(0),
            Kind::Full(bytes) => Some(bytes.as_ref().map_or(0, |b| b.len() as u64)),
            Kind::Stream { len, .. } => *len,
        }
    }

    #[must_use]
    pub const fn is_empty_body(&self) -> bool {
        matches!(self.0, Kind::Empty)
    }

    /// Reads the whole body into memory, failing once it exceeds `limit` bytes.
    ///
    /// # Errors
    ///
    /// [`CollectError::TooLarge`] past the limit; [`CollectError::Read`] when the stream fails.
    pub async fn collect_limited(mut self, limit: usize) -> Result<Bytes, CollectError> {
        use quench_core::stream::StreamExt;
        if let Some(len) = self.exact_len()
            && usize::try_from(len).map_or(true, |len| len > limit)
        {
            return Err(CollectError::TooLarge(limit));
        }
        let mut buf = Vec::new();
        while let Some(chunk) = self.next().await {
            let chunk = chunk.map_err(CollectError::Read)?;
            if buf.len() + chunk.len() > limit {
                return Err(CollectError::TooLarge(limit));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(Bytes::from(buf))
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body")
            .field("len", &self.exact_len())
            .finish_non_exhaustive()
    }
}

impl Stream for Body {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match &mut self.0 {
            Kind::Empty => Poll::Ready(None),
            Kind::Full(bytes) => Poll::Ready(bytes.take().map(Ok)),
            Kind::Stream { chunks, .. } => chunks.as_mut().poll_next(cx),
        }
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Vec<u8>> for Body {
    fn from(bytes: Vec<u8>) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<String> for Body {
    fn from(text: String) -> Self {
        Self::from_bytes(text)
    }
}

impl From<&'static str> for Body {
    fn from(text: &'static str) -> Self {
        Self::from_bytes(text)
    }
}

/// Erases the stream's error type. The inner stream is boxed so that pin
/// projection needs no `unsafe`.
struct MapErr<S>(Pin<Box<S>>);

impl<S, E> Stream for MapErr<S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0
            .as_mut()
            .poll_next(cx)
            .map(|item| item.map(|r| r.map_err(Into::into)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::stream::{self, StreamExt};

    #[test]
    fn fixed_bodies_know_their_length() {
        assert_eq!(Body::empty().exact_len(), Some(0));
        assert!(Body::empty().is_empty_body());
        assert_eq!(Body::default().exact_len(), Some(0));
        assert_eq!(Body::from_bytes("abc").exact_len(), Some(3));
        assert!(Body::from_bytes(Bytes::new()).is_empty_body());
        assert_eq!(Body::from(vec![1u8, 2]).exact_len(), Some(2));
        assert_eq!(Body::from(String::from("xy")).exact_len(), Some(2));
        assert_eq!(Body::from("x").exact_len(), Some(1));
        assert_eq!(Body::from(Bytes::from_static(b"four")).exact_len(), Some(4));
        assert!(format!("{:?}", Body::from("x")).contains("len"));
    }

    #[tokio::test]
    async fn full_bodies_yield_once() {
        let mut body = Body::from("once");
        assert_eq!(body.next().await.unwrap().unwrap(), "once");
        assert!(body.next().await.is_none());
        assert_eq!(body.exact_len(), Some(0));
        assert!(Body::empty().next().await.is_none());
    }

    #[tokio::test]
    async fn streams_have_no_length_unless_declared() {
        let items = || {
            vec![
                Ok::<_, std::io::Error>(Bytes::from_static(b"ab")),
                Ok(Bytes::from_static(b"c")),
            ]
        };
        let body = Body::from_stream(stream::iter(items()));
        assert_eq!(body.exact_len(), None);
        assert_eq!(body.collect_limited(10).await.unwrap(), "abc");
        let sized = Body::from_stream_sized(stream::iter(items()), 3);
        assert_eq!(sized.exact_len(), Some(3));
        assert_eq!(sized.collect_limited(10).await.unwrap(), "abc");
    }

    #[tokio::test]
    async fn collect_limited_enforces_the_limit_early_and_late() {
        // Known length: refused without reading.
        let err = Body::from("0123456789")
            .collect_limited(5)
            .await
            .unwrap_err();
        assert!(matches!(err, CollectError::TooLarge(5)));
        assert!(err.to_string().contains("exceeds 5"));
        // Unknown length: refused once the running total crosses the limit.
        let items = vec![
            Ok::<_, std::io::Error>(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"def")),
        ];
        let err = Body::from_stream(stream::iter(items))
            .collect_limited(4)
            .await
            .unwrap_err();
        assert!(matches!(err, CollectError::TooLarge(4)));
        // Exactly at the limit is fine.
        assert_eq!(
            Body::from("12345").collect_limited(5).await.unwrap(),
            "12345"
        );
    }

    #[tokio::test]
    async fn collect_limited_reports_stream_errors() {
        let items = vec![
            Ok(Bytes::from_static(b"ab")),
            Err(std::io::Error::other("lost")),
        ];
        let err = Body::from_stream(stream::iter(items))
            .collect_limited(10)
            .await
            .unwrap_err();
        assert!(matches!(err, CollectError::Read(_)));
        assert!(err.to_string().contains("lost"));
        let _: &dyn std::error::Error = &err;
    }
}
