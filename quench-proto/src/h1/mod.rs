//! HTTP/1.1: head parsing, body framing and the server and client connection drivers.

#[cfg(feature = "client")]
pub mod client;
pub mod decode;
pub mod parse;
pub mod reader;
#[cfg(feature = "server")]
pub mod server;

pub use parse::{Framing, Limits, ProtoError};

// The connection drivers share these helpers; without either there is nothing to share them with.
#[cfg(any(feature = "server", feature = "client"))]
mod io_util {
    use crate::error::Error;
    use crate::header::HeaderMap;
    use std::time::Duration;
    use tokio::io::{AsyncWrite, AsyncWriteExt};
    use tokio::time::timeout;

    /// The `Content-Length` a header map declares, if it parses.
    pub fn declared_length(headers: &HeaderMap) -> Option<u64> {
        headers
            .get("content-length")?
            .to_str()
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    /// Writes and flushes `data`, giving up after `limit`.
    pub async fn send<W: AsyncWrite + Unpin>(
        wr: &mut W,
        data: &[u8],
        limit: Duration,
    ) -> Result<(), Error> {
        timeout(limit, async {
            wr.write_all(data).await?;
            wr.flush().await
        })
        .await
        .map_err(|_| Error::Timeout("writing"))?
        .map_err(Error::Io)
    }
}

#[cfg(any(feature = "server", feature = "client"))]
pub(crate) use io_util::{declared_length, send};
