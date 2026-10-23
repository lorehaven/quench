use crate::error::ExtractError;
use quench_core::bytes::Bytes;
use quench_core::stream::Stream;
use quench_proto::body::{Body, BoxError, CollectError};
use std::pin::Pin;
use std::task::{Context, Poll};

/// An inbound request body.
///
/// Wraps a real connection or, in tests, an in-memory buffer - streamed rather than pre-buffered so handlers that
/// pipe large uploads to disk (blob/artifact storage) don't pay for a full
/// in-memory copy.
#[derive(Debug)]
pub struct InboundBody(Body);

impl InboundBody {
    #[must_use]
    pub const fn empty() -> Self {
        Self(Body::empty())
    }

    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self(Body::from_bytes(bytes))
    }

    /// Buffers the whole body into memory, up to `limit` bytes.
    pub async fn collect_limited(self, limit: usize) -> Result<Bytes, ExtractError> {
        self.0
            .collect_limited(limit)
            .await
            .map_err(|err| match err {
                CollectError::TooLarge(limit) => ExtractError::BodyTooLarge(limit),
                CollectError::Read(err) => ExtractError::BodyReadFailed(err.to_string()),
            })
    }

    /// The underlying body, for handing to code that works on the protocol layer.
    #[must_use]
    pub fn into_body(self) -> Body {
        self.0
    }
}

impl From<Body> for InboundBody {
    fn from(body: Body) -> Self {
        Self(body)
    }
}

impl Stream for InboundBody {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.0).poll_next(cx)
    }
}
