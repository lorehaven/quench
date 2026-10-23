//! The byte-stream abstraction connections run over, and the hand-off after an HTTP upgrade.

use quench_core::bytes::Bytes;
use quench_core::future::BoxFuture;
use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A bidirectional byte stream: a TCP socket, a TLS session, an in-memory pipe.
pub trait Io: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static> Io for T {}

/// The raw connection after a `101 Switching Protocols` response: bytes the
/// peer already sent past the HTTP head come first, then the live stream.
pub struct Upgraded {
    io: Box<dyn Io>,
    prefix: Bytes,
}

impl Upgraded {
    #[cfg_attr(not(feature = "server"), allow(dead_code))] // only the server driver hands out upgraded streams
    pub(crate) fn new(io: Box<dyn Io>, prefix: Bytes) -> Self {
        Self { io, prefix }
    }
}

impl fmt::Debug for Upgraded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Upgraded")
            .field("buffered", &self.prefix.len())
            .finish_non_exhaustive()
    }
}

impl AsyncRead for Upgraded {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.prefix.is_empty() {
            let n = self.prefix.len().min(buf.remaining());
            let chunk = self.prefix.split_to(n);
            buf.put_slice(&chunk);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Upgraded {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

/// A callback waiting for the connection to be upgraded.
#[cfg_attr(not(feature = "server"), allow(dead_code))] // only the server driver runs the hook
pub struct OnUpgrade(Box<dyn FnOnce(Upgraded) -> BoxFuture<'static, ()> + Send>);

impl OnUpgrade {
    pub(crate) fn new<F, Fut>(handler: F) -> Self
    where
        F: FnOnce(Upgraded) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        Self(Box::new(move |upgraded| Box::pin(handler(upgraded))))
    }

    #[cfg_attr(not(feature = "server"), allow(dead_code))]
    pub(crate) fn run(self, upgraded: Upgraded) -> BoxFuture<'static, ()> {
        (self.0)(upgraded)
    }
}

impl fmt::Debug for OnUpgrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OnUpgrade")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn upgraded_yields_the_buffered_prefix_before_the_stream() {
        let (mut peer, local) = tokio::io::duplex(64);
        let mut upgraded = Upgraded::new(Box::new(local), Bytes::from_static(b"early"));
        peer.write_all(b"-late").await.unwrap();
        let mut got = [0u8; 10];
        upgraded.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"early-late");
        upgraded.write_all(b"pong").await.unwrap();
        upgraded.flush().await.unwrap();
        let mut reply = [0u8; 4];
        peer.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"pong");
        upgraded.shutdown().await.unwrap();
        assert!(format!("{upgraded:?}").contains("Upgraded"));
    }

    #[tokio::test]
    async fn a_small_read_buffer_drains_the_prefix_in_pieces() {
        let (_peer, local) = tokio::io::duplex(8);
        let mut upgraded = Upgraded::new(Box::new(local), Bytes::from_static(b"abcdef"));
        let mut two = [0u8; 2];
        upgraded.read_exact(&mut two).await.unwrap();
        assert_eq!(&two, b"ab");
        let mut four = [0u8; 4];
        upgraded.read_exact(&mut four).await.unwrap();
        assert_eq!(&four, b"cdef");
    }

    #[tokio::test]
    async fn on_upgrade_runs_the_handler_with_the_stream() {
        let (_peer, local) = tokio::io::duplex(8);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let hook = OnUpgrade::new(move |up: Upgraded| async move {
            let _ = tx.send(up.prefix.len());
        });
        hook.run(Upgraded::new(Box::new(local), Bytes::from_static(b"xyz")))
            .await;
        assert_eq!(rx.await.unwrap(), 3);
        assert_eq!(format!("{:?}", OnUpgrade::new(|_| async {})), "OnUpgrade");
    }
}
