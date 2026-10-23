//! Tokio sources as [`Stream`]s, ready for [`Response::streaming`](crate::response::Response::streaming).

use quench_core::bytes::Bytes;
use quench_core::stream::{Stream, unfold};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

/// How much of a reader one chunk holds.
const READ_CHUNK: usize = 64 * 1024;

/// Everything sent into the channel, until every sender is gone.
#[must_use]
pub fn receiver_stream<T: Send + 'static>(
    receiver: mpsc::Receiver<T>,
) -> impl Stream<Item = T> + Send + Unpin + 'static {
    unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    })
}

/// A reader's bytes in 64 KiB pieces. A read error is yielded once and ends the stream.
#[must_use]
pub fn reader_stream<R: AsyncRead + Unpin + Send + 'static>(
    reader: R,
) -> impl Stream<Item = std::io::Result<Bytes>> + Send + Unpin + 'static {
    unfold(Some(reader), |reader| async move {
        let mut reader = reader?;
        let mut buffer = vec![0u8; READ_CHUNK];
        match reader.read(&mut buffer).await {
            Ok(0) => None,
            Ok(read) => {
                buffer.truncate(read);
                Some((Ok(Bytes::from(buffer)), Some(reader)))
            }
            Err(error) => Some((Err(error), None)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::stream::StreamExt;

    #[tokio::test]
    async fn a_receiver_streams_until_its_senders_are_gone() {
        let (tx, rx) = mpsc::channel(4);
        tx.send(1).await.unwrap();
        tx.send(2).await.unwrap();
        drop(tx);
        assert_eq!(receiver_stream(rx).collect_vec().await, vec![1, 2]);
    }

    #[tokio::test]
    async fn a_reader_streams_in_chunks_and_a_failure_ends_it() {
        struct Failing;
        impl AsyncRead for Failing {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _: &mut std::task::Context<'_>,
                _: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::other("boom")))
            }
        }

        let data = vec![7u8; READ_CHUNK + 10];
        let chunks = reader_stream(std::io::Cursor::new(data.clone())).collect_vec().await;
        let joined: Vec<u8> = chunks.into_iter().flat_map(|c| c.unwrap().to_vec()).collect();
        assert_eq!(joined, data);

        let mut failing = reader_stream(Failing);
        assert!(failing.next().await.unwrap().is_err());
        assert!(failing.next().await.is_none());
    }
}
