use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body as HttpBody, Frame};
use std::pin::Pin;
use std::task::{Context, Poll};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A boxed, type-erased inbound request body (real connection or, in tests,
/// an in-memory buffer) - streamed rather than pre-buffered so handlers that
/// pipe large uploads to disk (blob/artifact storage) don't pay for a full
/// in-memory copy.
pub struct InboundBody(Pin<Box<dyn HttpBody<Data = Bytes, Error = BoxError> + Send + Sync>>);

impl InboundBody {
    pub fn empty() -> Self {
        Self(Box::pin(
            Empty::new().map_err(|never: std::convert::Infallible| match never {}),
        ))
    }

    pub fn from_bytes(bytes: impl Into<Bytes>) -> Self {
        Self(Box::pin(
            Full::new(bytes.into()).map_err(|never: std::convert::Infallible| match never {}),
        ))
    }

    /// Buffers the whole body into memory, up to `limit` bytes.
    pub async fn collect_limited(self, limit: usize) -> Result<Bytes, crate::error::ExtractError> {
        use crate::error::ExtractError;

        let mut body = self.0;
        let mut buf = Vec::new();
        loop {
            let Some(frame) = futures_lite_next(&mut body).await else {
                break;
            };
            let frame = frame.map_err(|e| ExtractError::BodyReadFailed(e.to_string()))?;
            if let Ok(data) = frame.into_data() {
                if buf.len() + data.len() > limit {
                    return Err(ExtractError::BodyTooLarge(limit));
                }
                buf.extend_from_slice(&data);
            }
        }
        Ok(Bytes::from(buf))
    }
}

impl HttpBody for InboundBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        self.get_mut().0.as_mut().poll_frame(cx)
    }
}

impl From<hyper::body::Incoming> for InboundBody {
    fn from(incoming: hyper::body::Incoming) -> Self {
        Self(Box::pin(incoming.map_err(|e| Box::new(e) as BoxError)))
    }
}

/// Small `poll_frame`-based `.next()` without pulling in a full `StreamExt`
/// dependency for one call site.
async fn futures_lite_next(
    body: &mut Pin<Box<dyn HttpBody<Data = Bytes, Error = BoxError> + Send + Sync>>,
) -> Option<Result<Frame<Bytes>, BoxError>> {
    std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await
}
