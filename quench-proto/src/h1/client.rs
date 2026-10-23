//! The client side of an HTTP/1.1 connection: send one request, stream the response back.

use super::decode::{BodyDecoder, LAST_CHUNK, encode_chunk};
use super::parse::{
    Framing, Limits, find_head_end, parse_response_head, response_framing, wants_keep_alive,
    write_request_head,
};
use super::reader::Reader;
use super::{declared_length, send};
use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::header::HeaderValue;
use crate::io::Io;
use crate::message::{Request, Response};
use quench_core::bytes::Bytes;
use quench_core::stream::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::oneshot;
use tokio::time::timeout;

/// Timeouts and limits for one client exchange.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub limits: Limits,
    /// From sending the request to receiving the full response head.
    pub response_head_timeout: Duration,
    /// Per write call while sending the request.
    pub write_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            limits: Limits {
                max_head_bytes: 256 * 1024,
                ..Limits::default()
            },
            response_head_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
        }
    }
}

impl<I: Io> std::fmt::Debug for ClientConn<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConn")
            .field("reader", &self.reader)
            .finish_non_exhaustive()
    }
}

impl<I: Io> std::fmt::Debug for Exchange<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exchange")
            .field("response", &self.response)
            .finish_non_exhaustive()
    }
}

/// An established connection, ready to carry one request at a time.
pub struct ClientConn<I: Io> {
    reader: Reader<ReadHalf<I>>,
    wr: WriteHalf<I>,
}

impl<I: Io> ClientConn<I> {
    pub fn new(io: I) -> Self {
        let (rd, wr) = tokio::io::split(io);
        Self {
            reader: Reader::new(rd),
            wr,
        }
    }

    /// Whether an idle connection is still usable: false once the peer closed
    /// it (or sent bytes nobody asked for) while it sat in a pool.
    pub fn is_reusable(&mut self) -> bool {
        let mut cx = Context::from_waker(Waker::noop());
        matches!(self.reader.poll_fill(&mut cx), Poll::Pending) && self.reader.buffered().is_empty()
    }
}

/// A response whose body is still streaming.
pub struct Exchange<I: Io> {
    pub response: Response,
    /// Yields the connection once the body was read to its end and the peer
    /// allows reuse. Errors (sender dropped) when the connection must be discarded.
    pub reuse: oneshot::Receiver<ClientConn<I>>,
}

/// The `Host` value for a URI: its authority without userinfo.
fn host_header(uri: &crate::uri::Uri) -> Option<String> {
    let authority = uri.authority()?;
    Some(
        authority
            .rsplit_once('@')
            .map_or(authority, |(_, rest)| rest)
            .to_string(),
    )
}

/// Writes `request` on `conn` and reads the response head.
///
/// # Errors
///
/// Any [`Error`] from writing, parsing or the deadlines.
pub async fn send_request<I: Io>(
    mut conn: ClientConn<I>,
    request: Request,
    config: &ClientConfig,
) -> Result<Exchange<I>, Error> {
    let Request {
        method,
        uri,
        mut headers,
        mut body,
        ..
    } = request;
    if !headers.contains_key("host")
        && let Some(host) = host_header(&uri)
        && let Ok(value) = HeaderValue::from_str(&host)
    {
        headers.insert("host", value);
    }
    let sends_body =
        body.exact_len() != Some(0) || matches!(method.as_str(), "POST" | "PUT" | "PATCH");
    let framing = if let Some(n) = declared_length(&headers) {
        headers.remove("transfer-encoding");
        Framing::Length(n)
    } else if let Some(n) = body.exact_len() {
        headers.remove("transfer-encoding");
        if sends_body {
            headers.insert("content-length", HeaderValue::from(n));
        }
        Framing::Length(n)
    } else {
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        Framing::Chunked
    };

    let mut out = Vec::with_capacity(512);
    write_request_head(&mut out, &method, uri.path_and_query(), &headers);
    let mut sent = 0u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(Error::Body)?;
        match framing {
            Framing::Length(n) => {
                sent += chunk.len() as u64;
                if sent > n {
                    return Err(Error::BodyLength);
                }
                out.extend_from_slice(&chunk);
            }
            _ => encode_chunk(&mut out, &chunk),
        }
        if out.len() >= 16 * 1024 {
            send(&mut conn.wr, &out, config.write_timeout).await?;
            out.clear();
        }
    }
    match framing {
        Framing::Length(n) if sent != n => return Err(Error::BodyLength),
        Framing::Chunked => out.extend_from_slice(LAST_CHUNK),
        _ => {}
    }
    send(&mut conn.wr, &out, config.write_timeout).await?;

    let head = timeout(
        config.response_head_timeout,
        read_response_head(&mut conn.reader, config),
    )
    .await
    .map_err(|_| Error::Timeout("waiting for the response head"))??;

    let framing = response_framing(&method, head.status, &head.headers)?;
    let keep_alive =
        wants_keep_alive(head.version, &head.headers) && framing != Framing::UntilClose;
    let (reuse_tx, reuse) = oneshot::channel();
    let body = if framing == Framing::None {
        if keep_alive {
            let _ = reuse_tx.send(conn);
        }
        Body::empty()
    } else {
        let stream = ClientBody {
            conn: Some(conn),
            decoder: BodyDecoder::new(framing),
            reuse: keep_alive.then_some(reuse_tx),
        };
        match framing {
            Framing::Length(n) => Body::from_stream_sized(stream, n),
            _ => Body::from_stream(stream),
        }
    };
    let response = Response {
        status: head.status,
        version: head.version,
        headers: head.headers,
        extensions: crate::extensions::Extensions::new(),
        body,
        upgrade: None,
    };
    Ok(Exchange { response, reuse })
}

/// Reads response heads, skipping interim `1xx` responses.
async fn read_response_head<I: Io>(
    reader: &mut Reader<ReadHalf<I>>,
    config: &ClientConfig,
) -> Result<super::parse::ResponseHead, Error> {
    loop {
        let end = loop {
            if let Some(end) = find_head_end(reader.buffered()) {
                break end;
            }
            if reader.buffered().len() > config.limits.max_head_bytes {
                return Err(Error::Proto(super::parse::ProtoError::HeadersTooLarge));
            }
            if reader.fill().await? == 0 {
                return Err(Error::UnexpectedEof);
            }
        };
        let head = parse_response_head(&reader.buffered()[..end], &config.limits)?;
        reader.consume(end);
        if head.status.is_informational() {
            continue;
        }
        return Ok(head);
    }
}

struct ClientBody<I: Io> {
    conn: Option<ClientConn<I>>,
    decoder: BodyDecoder,
    reuse: Option<oneshot::Sender<ClientConn<I>>>,
}

impl<I: Io> Stream for ClientBody<I> {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        let Some(conn) = this.conn.as_mut() else {
            return Poll::Ready(None);
        };
        match conn.reader.poll_body(cx, &mut this.decoder) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(Some(chunk))) => Poll::Ready(Some(Ok(chunk))),
            Poll::Ready(Ok(None)) => {
                if let (Some(conn), Some(tx)) = (this.conn.take(), this.reuse.take()) {
                    let _ = tx.send(conn);
                }
                this.conn = None;
                Poll::Ready(None)
            }
            Poll::Ready(Err(err)) => {
                this.conn = None;
                this.reuse = None;
                Poll::Ready(Some(Err(Box::new(err))))
            }
        }
    }
}
