//! A buffered, poll-based reader that pulls heads and body chunks off a byte stream.

use super::decode::BodyDecoder;
use crate::error::Error;
use quench_core::bytes::Bytes;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, ReadBuf};

const READ_SIZE: usize = 16 * 1024;
/// Consumed prefix length past which the buffer is compacted.
const COMPACT_AT: usize = 64 * 1024;

pub struct Reader<R> {
    rd: R,
    buf: Vec<u8>,
    pos: usize,
}

impl<R> std::fmt::Debug for Reader<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("buffered", &(self.buf.len() - self.pos))
            .finish_non_exhaustive()
    }
}

impl<R: AsyncRead + Unpin> Reader<R> {
    pub const fn new(rd: R) -> Self {
        Self {
            rd,
            buf: Vec::new(),
            pos: 0,
        }
    }

    /// Bytes read but not yet consumed.
    pub fn buffered(&self) -> &[u8] {
        &self.buf[self.pos..]
    }

    pub fn consume(&mut self, n: usize) {
        self.pos += n;
        if self.pos == self.buf.len() {
            self.buf.clear();
            self.pos = 0;
        } else if self.pos >= COMPACT_AT {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    /// Takes everything buffered, for handing to an upgraded connection.
    pub fn take_buffered(&mut self) -> Bytes {
        let rest = Bytes::from(self.buffered().to_vec());
        self.buf.clear();
        self.pos = 0;
        rest
    }

    pub fn into_inner(self) -> R {
        self.rd
    }

    /// Reads more bytes into the buffer; `Ready(Ok(0))` means the peer closed.
    pub fn poll_fill(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let mut chunk = [0u8; READ_SIZE];
        let mut read_buf = ReadBuf::new(&mut chunk);
        ready!(Pin::new(&mut self.rd).poll_read(cx, &mut read_buf))?;
        let filled = read_buf.filled();
        self.buf.extend_from_slice(filled);
        Poll::Ready(Ok(filled.len()))
    }

    pub async fn fill(&mut self) -> io::Result<usize> {
        std::future::poll_fn(|cx| self.poll_fill(cx)).await
    }

    /// The next piece of body, `None` once the message is complete.
    ///
    /// Cancel-safe: state only advances when a piece is returned or bytes were consumed.
    pub fn poll_body(
        &mut self,
        cx: &mut Context<'_>,
        decoder: &mut BodyDecoder,
    ) -> Poll<Result<Option<Bytes>, Error>> {
        loop {
            if decoder.is_done() {
                return Poll::Ready(Ok(None));
            }
            if !self.buffered().is_empty() {
                let mut out = Vec::new();
                let used = decoder.feed(self.buffered(), &mut out)?;
                self.consume(used);
                if !out.is_empty() {
                    return Poll::Ready(Ok(Some(Bytes::from(out))));
                }
                continue;
            }
            match ready!(self.poll_fill(cx))? {
                0 if decoder.reads_to_eof() => return Poll::Ready(Ok(None)),
                0 => return Poll::Ready(Err(Error::UnexpectedEof)),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::h1::parse::Framing;
    use tokio::io::AsyncWriteExt;

    async fn next(
        reader: &mut Reader<tokio::io::DuplexStream>,
        decoder: &mut BodyDecoder,
    ) -> Result<Option<Bytes>, Error> {
        std::future::poll_fn(|cx| reader.poll_body(cx, decoder)).await
    }

    #[quench_test::test]
    async fn reads_a_fixed_body_and_leaves_pipelined_bytes() {
        let (mut peer, local) = tokio::io::duplex(1024);
        peer.write_all(b"helloNEXT").await.unwrap();
        let mut reader = Reader::new(local);
        let mut decoder = BodyDecoder::new(Framing::Length(5));
        assert_eq!(
            next(&mut reader, &mut decoder).await.unwrap().unwrap(),
            "hello"
        );
        assert!(next(&mut reader, &mut decoder).await.unwrap().is_none());
        assert_eq!(reader.buffered(), b"NEXT");
        assert_eq!(&reader.take_buffered()[..], b"NEXT");
        assert!(reader.buffered().is_empty());
    }

    #[quench_test::test]
    async fn a_short_body_is_an_unexpected_eof() {
        let (mut peer, local) = tokio::io::duplex(1024);
        peer.write_all(b"abc").await.unwrap();
        drop(peer);
        let mut reader = Reader::new(local);
        let mut decoder = BodyDecoder::new(Framing::Length(10));
        assert_eq!(
            next(&mut reader, &mut decoder).await.unwrap().unwrap(),
            "abc"
        );
        assert!(matches!(
            next(&mut reader, &mut decoder).await,
            Err(Error::UnexpectedEof)
        ));
    }

    #[quench_test::test]
    async fn read_to_eof_bodies_end_on_close() {
        let (mut peer, local) = tokio::io::duplex(1024);
        peer.write_all(b"all of it").await.unwrap();
        drop(peer);
        let mut reader = Reader::new(local);
        let mut decoder = BodyDecoder::new(Framing::UntilClose);
        assert_eq!(
            next(&mut reader, &mut decoder).await.unwrap().unwrap(),
            "all of it"
        );
        assert!(next(&mut reader, &mut decoder).await.unwrap().is_none());
    }

    #[quench_test::test]
    async fn chunked_bodies_surface_decoder_errors() {
        let (mut peer, local) = tokio::io::duplex(1024);
        peer.write_all(b"zz\r\n").await.unwrap();
        let mut reader = Reader::new(local);
        let mut decoder = BodyDecoder::new(Framing::Chunked);
        assert!(matches!(
            next(&mut reader, &mut decoder).await,
            Err(Error::Proto(_))
        ));
    }

    #[quench_test::test]
    async fn consume_compacts_a_large_consumed_prefix() {
        let (mut peer, local) = tokio::io::duplex(256 * 1024);
        peer.write_all(&vec![b'x'; 80 * 1024]).await.unwrap();
        peer.write_all(b"tail").await.unwrap();
        drop(peer);
        let mut reader = Reader::new(local);
        while reader.fill().await.unwrap() > 0 {}
        let total = reader.buffered().len();
        reader.consume(COMPACT_AT + 10);
        assert_eq!(reader.buffered().len(), total - COMPACT_AT - 10);
        assert!(reader.buffered().ends_with(b"tail"));
        let _inner = reader.into_inner();
    }
}
