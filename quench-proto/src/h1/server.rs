//! The server side of an HTTP/1.1 connection: read a request, run the service,
//! write the response, repeat while the connection stays reusable.

use super::decode::{BodyDecoder, LAST_CHUNK, encode_chunk};
use super::parse::{
    Framing, Limits, RequestHead, find_head_end, has_token, parse_request_head, request_framing,
    wants_keep_alive, write_response_head,
};
use super::reader::Reader;
use super::{declared_length, send};
use crate::body::{Body, BoxError};
use crate::error::Error;
use crate::header::HeaderValue;
use crate::io::{Io, OnUpgrade, Upgraded};
use crate::message::{Request, Response};
use crate::method::Method;
use crate::status::StatusCode;
use crate::version::Version;
use quench_core::bytes::Bytes;
use quench_core::future::BoxFuture;
use quench_core::stream::{Stream, StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, timeout, timeout_at};

/// Responses up to this size are written in the same syscall as their head.
const COALESCE_LIMIT: usize = 16 * 1024;

/// Timeouts and limits for one server connection.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub limits: Limits,
    /// From the first byte of a request to the end of its head ("slowloris" guard).
    pub header_read_timeout: Duration,
    /// How long a keep-alive connection may sit between requests.
    pub idle_timeout: Duration,
    /// Per write call: a client that stops reading cannot hold a connection forever.
    pub write_timeout: Duration,
    /// Unread request-body bytes discarded so the response is not lost to a reset.
    pub drain_limit: u64,
    /// How long that discarding may take.
    pub drain_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            header_read_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(75),
            write_timeout: Duration::from_secs(60),
            drain_limit: 1024 * 1024,
            drain_timeout: Duration::from_secs(2),
        }
    }
}

/// Something that turns requests into responses.
pub trait Service: Send + Sync {
    fn call(&self, req: Request) -> BoxFuture<'_, Response>;
}

impl<F, Fut> Service for F
where
    F: Fn(Request) -> Fut + Send + Sync,
    Fut: Future<Output = Response> + Send + 'static,
{
    fn call(&self, req: Request) -> BoxFuture<'_, Response> {
        Box::pin(self(req))
    }
}

/// Tells connections to finish their current request and stop.
#[derive(Debug)]
pub struct ShutdownHandle(watch::Sender<bool>);

/// The receiving side of a [`ShutdownHandle`].
#[derive(Clone, Debug)]
pub struct Shutdown {
    rx: watch::Receiver<bool>,
    // Keeps the channel open for `Shutdown::never`.
    _keep: Option<Arc<watch::Sender<bool>>>,
}

#[must_use]
pub fn shutdown_channel() -> (ShutdownHandle, Shutdown) {
    let (tx, rx) = watch::channel(false);
    (ShutdownHandle(tx), Shutdown { rx, _keep: None })
}

impl ShutdownHandle {
    pub fn signal(&self) {
        self.0.send_replace(true);
    }
}

impl Shutdown {
    /// A signal that never fires.
    #[must_use]
    pub fn never() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            rx,
            _keep: Some(Arc::new(tx)),
        }
    }

    #[must_use]
    pub fn is_set(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves once the signal fires; pends forever if the handle is dropped unsignalled.
    pub async fn wait(&mut self) {
        loop {
            if self.is_set() {
                return;
            }
            if self.rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

enum Next {
    KeepAlive,
    Close,
    Upgrade(OnUpgrade),
}

/// Serves requests on `io` until the peer closes, a protocol error occurs, or `shutdown` fires.
///
/// # Errors
///
/// The reason the connection ended abnormally. A peer that simply closes
/// between requests, or an idle timeout, is `Ok`.
pub async fn serve_connection<I, S>(
    io: I,
    service: &S,
    config: &ServerConfig,
    mut shutdown: Shutdown,
) -> Result<(), Error>
where
    I: Io,
    S: Service + ?Sized,
{
    let (rd, mut wr) = tokio::io::split(io);
    let mut reader = Reader::new(rd);
    loop {
        let head = match read_request_head(&mut reader, config, &mut shutdown).await {
            Ok(Some(head)) => head,
            Ok(None) => return Ok(()),
            Err(Error::Proto(err)) => {
                respond_error(&mut wr, err.status(), config).await;
                return Err(Error::Proto(err));
            }
            Err(err) => return Err(err),
        };
        match serve_one(&mut reader, &mut wr, head, service, config, &shutdown).await? {
            Next::KeepAlive => {}
            Next::Close => {
                let _ = wr.shutdown().await;
                return Ok(());
            }
            Next::Upgrade(hook) => {
                let prefix = reader.take_buffered();
                let io = reader.into_inner().unsplit(wr);
                hook.run(Upgraded::new(Box::new(io), prefix)).await;
                return Ok(());
            }
        }
    }
}

async fn read_request_head<R>(
    reader: &mut Reader<R>,
    config: &ServerConfig,
    shutdown: &mut Shutdown,
) -> Result<Option<RequestHead>, Error>
where
    R: AsyncRead + Unpin,
{
    let mut deadline: Option<Instant> = None;
    let mut scanned = 0usize;
    loop {
        // Tolerate blank lines between pipelined requests (RFC 9112 section 2.2).
        let leading = reader
            .buffered()
            .iter()
            .take_while(|&&b| b == b'\r' || b == b'\n')
            .count();
        if leading > 0 {
            reader.consume(leading);
            scanned = 0;
        }
        let from = scanned.saturating_sub(3);
        if let Some(end) = find_head_end(&reader.buffered()[from..]) {
            let end = from + end;
            let head = parse_request_head(&reader.buffered()[..end], &config.limits)?;
            reader.consume(end);
            return Ok(Some(head));
        }
        scanned = reader.buffered().len();
        if scanned > config.limits.max_head_bytes {
            return Err(Error::Proto(super::parse::ProtoError::HeadersTooLarge));
        }
        let read = if reader.buffered().is_empty() {
            tokio::select! {
                r = timeout(config.idle_timeout, reader.fill()) => match r {
                    Ok(r) => r?,
                    Err(_) => return Ok(None),
                },
                () = shutdown.wait() => return Ok(None),
            }
        } else {
            let deadline =
                *deadline.get_or_insert_with(|| Instant::now() + config.header_read_timeout);
            timeout_at(deadline, reader.fill())
                .await
                .map_err(|_| Error::Timeout("reading the request head"))??
        };
        if read == 0 {
            return if reader.buffered().is_empty() {
                Ok(None)
            } else {
                Err(Error::UnexpectedEof)
            };
        }
    }
}

/// State shared between the body pump and the connection loop.
#[derive(Default)]
struct Flags {
    /// The receiver of the request body was dropped.
    body_dropped: AtomicBool,
    /// The whole request body has been read off the wire.
    wire_complete: AtomicBool,
    /// Reading the request body failed or exceeded the drain limit.
    broken: AtomicBool,
}

struct RxBody {
    rx: mpsc::Receiver<Result<Bytes, BoxError>>,
    flags: Arc<Flags>,
}

impl Stream for RxBody {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for RxBody {
    fn drop(&mut self) {
        self.flags.body_dropped.store(true, Ordering::SeqCst);
    }
}

/// Moves the request body from the wire to the handler. Once the handler drops
/// its end, keeps reading and discards, up to the drain limit, so the
/// connection can be reused (or at least closed without a reset).
async fn pump_body<R>(
    reader: &mut Reader<R>,
    decoder: &mut BodyDecoder,
    tx: mpsc::Sender<Result<Bytes, BoxError>>,
    flags: &Flags,
    drain_limit: u64,
) where
    R: AsyncRead + Unpin,
{
    let mut discarded = 0u64;
    let mut delivering = true;
    loop {
        match std::future::poll_fn(|cx| reader.poll_body(cx, decoder)).await {
            Ok(Some(chunk)) => {
                if decoder.is_done() {
                    flags.wire_complete.store(true, Ordering::SeqCst);
                }
                if delivering && tx.send(Ok(chunk.clone())).await.is_err() {
                    delivering = false;
                }
                if !delivering {
                    discarded += chunk.len() as u64;
                    if discarded > drain_limit {
                        flags.broken.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            }
            Ok(None) => {
                flags.wire_complete.store(true, Ordering::SeqCst);
                return;
            }
            Err(err) => {
                flags.broken.store(true, Ordering::SeqCst);
                if delivering {
                    let _ = tx.send(Err(Box::new(err))).await;
                }
                return;
            }
        }
    }
}

async fn serve_one<R, W, S>(
    reader: &mut Reader<R>,
    wr: &mut W,
    head: RequestHead,
    service: &S,
    config: &ServerConfig,
    shutdown: &Shutdown,
) -> Result<Next, Error>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    S: Service + ?Sized,
{
    let framing = match request_framing(head.version, &head.headers) {
        Ok(framing) => framing,
        Err(err) => {
            respond_error(wr, err.status(), config).await;
            return Err(Error::Proto(err));
        }
    };
    if let Some(expect) = head.headers.get_joined("expect") {
        if !expect.trim().eq_ignore_ascii_case("100-continue") {
            respond_error(wr, StatusCode::EXPECTATION_FAILED, config).await;
            return Ok(Next::Close);
        }
        if framing != Framing::None && head.version == Version::HTTP_11 {
            send(wr, b"HTTP/1.1 100 Continue\r\n\r\n", config.write_timeout).await?;
        }
    }
    let keep_alive_wanted = wants_keep_alive(head.version, &head.headers);
    let method = head.method.clone();
    let version = head.version;

    let flags = Arc::new(Flags::default());
    let (tx, rx) = mpsc::channel(1);
    let body = match framing {
        Framing::None => Body::empty(),
        Framing::Length(n) => Body::from_stream_sized(
            RxBody {
                rx,
                flags: flags.clone(),
            },
            n,
        ),
        _ => Body::from_stream(RxBody {
            rx,
            flags: flags.clone(),
        }),
    };
    if framing == Framing::None {
        flags.wire_complete.store(true, Ordering::SeqCst);
    }
    let request = Request {
        method: head.method,
        uri: head.uri,
        version: head.version,
        headers: head.headers,
        extensions: crate::extensions::Extensions::new(),
        body,
    };

    let mut decoder = BodyDecoder::new(framing);
    let mut pump = std::pin::pin!(pump_body(
        reader,
        &mut decoder,
        tx,
        &flags,
        config.drain_limit
    ));
    let mut pump_running = framing != Framing::None;
    let mut handler = std::pin::pin!(service.call(request));

    let response = loop {
        tokio::select! {
            response = &mut handler => break response,
            () = &mut pump, if pump_running => pump_running = false,
        }
    };
    // Give the pump one chance to notice a dropped body and discard what is already buffered.
    if pump_running {
        tokio::select! {
            biased;
            () = &mut pump => pump_running = false,
            () = std::future::ready(()) => {}
        }
    }

    let body_clean =
        flags.wire_complete.load(Ordering::SeqCst) && !flags.broken.load(Ordering::SeqCst);
    let keep_alive = keep_alive_wanted && body_clean && !shutdown.is_set();
    let write = write_response(wr, response, &method, version, keep_alive, config);
    let mut write = std::pin::pin!(write);
    let written = loop {
        tokio::select! {
            written = &mut write => break written?,
            () = &mut pump, if pump_running => pump_running = false,
        }
    };
    if pump_running {
        // Linger: let the client finish sending so it can read the response before the close.
        let _ = timeout(config.drain_timeout, &mut pump).await;
    }

    if let Some(hook) = written.upgrade {
        return Ok(Next::Upgrade(hook));
    }
    let reusable = written.keep_alive
        && flags.wire_complete.load(Ordering::SeqCst)
        && !flags.broken.load(Ordering::SeqCst);
    Ok(if reusable {
        Next::KeepAlive
    } else {
        Next::Close
    })
}

struct Written {
    keep_alive: bool,
    upgrade: Option<OnUpgrade>,
}

async fn write_response<W>(
    wr: &mut W,
    mut response: Response,
    method: &Method,
    request_version: Version,
    mut keep_alive: bool,
    config: &ServerConfig,
) -> Result<Written, Error>
where
    W: AsyncWrite + Unpin,
{
    let upgrade = response.upgrade.take();
    let status = response.status;
    let head_request = *method == Method::HEAD;
    let bodyless_status = status.has_no_body() || upgrade.is_some();
    let mut headers = std::mem::take(&mut response.headers);
    if has_token(&headers, "connection", "close") {
        keep_alive = false;
    }

    let framing = if bodyless_status {
        if status.is_informational() || status == StatusCode::NO_CONTENT {
            headers.remove("content-length");
        }
        headers.remove("transfer-encoding");
        Framing::None
    } else if let Some(n) = declared_length(&headers) {
        headers.remove("transfer-encoding");
        Framing::Length(n)
    } else if let Some(n) = response.body.exact_len() {
        headers.remove("transfer-encoding");
        headers.insert("content-length", HeaderValue::from(n));
        Framing::Length(n)
    } else if request_version == Version::HTTP_11 {
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        Framing::Chunked
    } else {
        keep_alive = false;
        Framing::UntilClose
    };
    if upgrade.is_some() {
        keep_alive = false;
    } else if !headers.contains_key("date") {
        headers.insert(
            "date",
            HeaderValue::from_str(&crate::date::now())
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        );
    }
    if upgrade.is_none() {
        if !keep_alive {
            headers.insert("connection", HeaderValue::from_static("close"));
        } else if request_version == Version::HTTP_10 {
            headers.insert("connection", HeaderValue::from_static("keep-alive"));
        }
    }

    let mut out = Vec::with_capacity(512);
    write_response_head(&mut out, status, &headers);
    let send_body = !head_request && !matches!(framing, Framing::None);
    if !send_body {
        send(wr, &out, config.write_timeout).await?;
        return Ok(Written {
            keep_alive,
            upgrade,
        });
    }

    write_body(wr, out, response.body, framing, config).await?;
    Ok(Written {
        keep_alive,
        upgrade,
    })
}

/// Writes the body after the head (which is still in `out`), framed as `framing` says.
async fn write_body<W>(
    wr: &mut W,
    mut out: Vec<u8>,
    mut body: Body,
    framing: Framing,
    config: &ServerConfig,
) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    let mut sent = 0u64;
    // A streaming body can take a long while to produce its first chunk (a server-sent event
    // stream, say). Unless that chunk is ready now, send the head first: the client then knows the
    // response has begun, and a browser's `EventSource` opens on it.
    let mut first = None;
    if matches!(framing, Framing::Chunked | Framing::UntilClose) {
        let mut next = std::pin::pin!(body.next());
        match poll_now(next.as_mut()) {
            Poll::Ready(item) => first = Some(item),
            Poll::Pending => {
                send(wr, &out, config.write_timeout).await?;
                out.clear();
                first = Some(next.await);
            }
        }
    }
    loop {
        let item = match first.take() {
            Some(item) => item,
            None => body.next().await,
        };
        let Some(chunk) = item else { break };
        let chunk = chunk.map_err(Error::Body)?;
        match framing {
            Framing::Length(n) => {
                sent += chunk.len() as u64;
                if sent > n {
                    return Err(Error::BodyLength);
                }
                if out.len() + chunk.len() <= COALESCE_LIMIT {
                    out.extend_from_slice(&chunk);
                } else {
                    send(wr, &out, config.write_timeout).await?;
                    out.clear();
                    send(wr, &chunk, config.write_timeout).await?;
                    continue;
                }
            }
            Framing::Chunked => encode_chunk(&mut out, &chunk),
            _ => out.extend_from_slice(&chunk),
        }
        if matches!(framing, Framing::Chunked | Framing::UntilClose) || out.len() >= COALESCE_LIMIT
        {
            send(wr, &out, config.write_timeout).await?;
            out.clear();
        }
    }
    match framing {
        Framing::Length(n) if sent != n => return Err(Error::BodyLength),
        Framing::Chunked => out.extend_from_slice(LAST_CHUNK),
        _ => {}
    }
    if !out.is_empty() {
        send(wr, &out, config.write_timeout).await?;
    }
    Ok(())
}

/// Polls `future` once, with a waker that does nothing, to see whether it is ready right now. A
/// future polled this way and left pending must be polled again with a real waker, which every
/// future supports. Its own function so the context never lives across an `.await`.
fn poll_now<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(std::task::Waker::noop()))
}

/// A minimal bodiless answer for a request that could not be served; best effort.
async fn respond_error<W: AsyncWrite + Unpin>(
    wr: &mut W,
    status: StatusCode,
    config: &ServerConfig,
) {
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-length: 0\r\nconnection: close\r\ndate: {}\r\n\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or(""),
        crate::date::now()
    );
    let _ = send(wr, head.as_bytes(), config.write_timeout).await;
    let _ = wr.shutdown().await;
}
