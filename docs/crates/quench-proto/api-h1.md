---
title: API reference - HTTP/1.1
parent: quench-proto
nav_order: 4
---

Module `h1`. `h1::parse`, `h1::decode`, `h1::reader` are always built; `h1::server` needs feature `server`; `h1::client` needs `client`. Re-exports in `h1`: `Framing`, `Limits`, `ProtoError` (from `parse`).

## `h1::parse`

```rust
#[derive(Debug, Clone, Copy)]
pub struct Limits { pub max_head_bytes: usize, pub max_headers: usize, pub max_uri_bytes: usize }
impl Default for Limits;    // 64 * 1024, 100, 8 * 1024

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError { BadRequest(&'static str), UriTooLong, HeadersTooLarge, VersionNotSupported, NotImplemented }
impl ProtoError { #[must_use] pub const fn status(&self) -> StatusCode; }   // 400, 414, 431, 505, 501
// Display, std::error::Error

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing { None, Length(u64), Chunked, UntilClose }

#[derive(Debug)] pub struct RequestHead  { pub method: Method, pub uri: Uri, pub version: Version, pub headers: HeaderMap }
#[derive(Debug)] pub struct ResponseHead { pub status: StatusCode, pub version: Version, pub headers: HeaderMap }

#[must_use] pub fn find_head_end(buf: &[u8]) -> Option<usize>;
pub fn parse_request_head(head: &[u8], limits: &Limits) -> Result<RequestHead, ProtoError>;
pub fn parse_response_head(head: &[u8], limits: &Limits) -> Result<ResponseHead, ProtoError>;
pub fn request_framing(version: Version, headers: &HeaderMap) -> Result<Framing, ProtoError>;
pub fn response_framing(request_method: &Method, status: StatusCode, headers: &HeaderMap) -> Result<Framing, ProtoError>;
#[must_use] pub fn wants_keep_alive(version: Version, headers: &HeaderMap) -> bool;
#[must_use] pub fn has_token(headers: &HeaderMap, name: &str, token: &str) -> bool;
pub fn write_response_head(out: &mut Vec<u8>, status: StatusCode, headers: &HeaderMap);
pub fn write_request_head(out: &mut Vec<u8>, method: &Method, target: &str, headers: &HeaderMap);
```

* `ProtoError` Display: `malformed HTTP message: {why}`, `request target too long`, `message head too large`, `HTTP version not supported`, `transfer coding not implemented`.
* `find_head_end`: the index just past the first `\r\n\r\n`, if the buffer holds a whole head.
* `parse_request_head(head, limits)`: `head` includes the closing blank line. Checks, in order: total length vs `max_head_bytes` (`HeadersTooLarge`); line structure (see [Internals](internals.md#head-parsing-rules)); request line of exactly three space-separated parts; target length vs `max_uri_bytes` (`UriTooLong`); method token; target parses as a `Uri`; version `HTTP/1.1` or `HTTP/1.0` (any other `HTTP/...` is `VersionNotSupported`; anything else `BadRequest("bad HTTP version")`); header count vs `max_headers`; for HTTP/1.1 exactly one `Host`.
* `parse_response_head`: status line `HTTP/1.x SP code [SP reason]`; the reason phrase is discarded; same header rules; no `Host` rule.
* `request_framing(version, headers)`: `Transfer-Encoding` present -> must be exactly `chunked` (HTTP/1.0 or together with `Content-Length` is `BadRequest`; `..., chunked` with other codings is `NotImplemented`; not ending in chunked is `BadRequest`) -> `Chunked`. Otherwise `Content-Length` 0 or absent -> `None`, `n` -> `Length(n)`.
* `response_framing(method, status, headers)`: `None` for a response to `HEAD`, for 1xx/204/304 and for a successful `CONNECT`; a `Transfer-Encoding` that is exactly `chunked` -> `Chunked` (even if `Content-Length` is also present); else `Content-Length` 0 -> `None`, `n` -> `Length(n)`, absent -> `UntilClose`.
* `wants_keep_alive`: false when any `Connection` token is `close`; otherwise true for HTTP/1.1 or when a `keep-alive` token is present.
* `has_token`: case-insensitive token search across all lines of the header.
* `write_*_head`: append `HTTP/1.1 <code> <reason>\r\n` / `<METHOD> <target> HTTP/1.1\r\n`, then each header as `name: value\r\n`, then `\r\n`. The response always says `HTTP/1.1`, whatever the request version.

## `h1::decode`

```rust
#[derive(Debug)] pub struct BodyDecoder { /* kind */ }
impl BodyDecoder {
    #[must_use] pub const fn new(framing: Framing) -> Self;
    #[must_use] pub const fn is_done(&self) -> bool;
    #[must_use] pub const fn reads_to_eof(&self) -> bool;
    #[must_use] pub const fn remaining(&self) -> Option<u64>;
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<usize, ProtoError>;
}
pub fn encode_chunk(out: &mut Vec<u8>, data: &[u8]);
pub const LAST_CHUNK: &[u8] = b"0\r\n\r\n";
```

* `feed`: consumes what it can of `input`, appends payload to `out`, returns the number of input bytes used; call again with more input until `is_done()`. Never consumes past the end of the message (pipelined bytes stay unconsumed). Errors (`ProtoError::BadRequest`): `bad chunk size` (empty, more than 16 hex digits, non-hex), `chunk data not followed by CRLF`, `chunk line too long` (a size or trailer line over 4096 bytes), `trailers too large` (over 16 KiB of trailer lines), `bare line feed in chunked body`, `bad trailer line` (CR, LF or NUL inside a trailer line). Chunk extensions (`;...`) are ignored; trailers are read and discarded.
* `is_done`: the message is complete (never true for read-to-EOF). `reads_to_eof`: framing is `UntilClose`. `remaining`: bytes still owed for a fixed-length body, else `None`.
* `encode_chunk`: writes `<hex length>\r\n<data>\r\n`; empty data writes nothing (a zero-length chunk would end the body). `LAST_CHUNK` ends a chunked body without trailers.

## `h1::reader`

```rust
pub struct Reader<R> { /* rd, buf, pos */ }                   // Debug
impl<R: AsyncRead + Unpin> Reader<R> {
    pub const fn new(rd: R) -> Self;
    pub fn buffered(&self) -> &[u8];
    pub fn consume(&mut self, n: usize);
    pub fn take_buffered(&mut self) -> Bytes;
    pub fn into_inner(self) -> R;
    pub fn poll_fill(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>>;     // Ready(Ok(0)) = peer closed
    pub async fn fill(&mut self) -> io::Result<usize>;
    pub fn poll_body(&mut self, cx: &mut Context<'_>, decoder: &mut BodyDecoder) -> Poll<Result<Option<Bytes>, Error>>;
}
```

A buffered reader that pulls heads and body pieces off a byte stream. `poll_body` is cancel-safe: state advances only when a piece is returned or bytes were consumed. For `UntilClose` bodies the end of the stream completes the body; a closed stream in the middle of a length or chunked body is `Error::UnexpectedEof`.

## `h1::server` (feature `server`)

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub limits: Limits,
    pub header_read_timeout: Duration,
    pub idle_timeout: Duration,
    pub write_timeout: Duration,
    pub drain_limit: u64,
    pub drain_timeout: Duration,
}
impl Default for ServerConfig;   // see Configuration

pub trait Service: Send + Sync { fn call(&self, req: Request) -> BoxFuture<'_, Response>; }
impl<F, Fut> Service for F where F: Fn(Request) -> Fut + Send + Sync, Fut: Future<Output = Response> + Send + 'static;

#[derive(Debug)] pub struct ShutdownHandle(/* watch::Sender<bool> */);
impl ShutdownHandle { pub fn signal(&self); }
#[derive(Clone, Debug)] pub struct Shutdown { /* watch::Receiver<bool> */ }
impl Shutdown {
    #[must_use] pub fn never() -> Self;
    #[must_use] pub fn is_set(&self) -> bool;
    pub async fn wait(&mut self);                 // resolves when signalled; pends forever if the handle is dropped unsignalled
}
#[must_use] pub fn shutdown_channel() -> (ShutdownHandle, Shutdown);

pub async fn serve_connection<I, S>(io: I, service: &S, config: &ServerConfig, shutdown: Shutdown) -> Result<(), Error>
    where I: Io, S: Service + ?Sized;
```

(`BoxFuture` is `quench_core`'s boxed future type.)

`serve_connection` loop, per request:

1. Read the head (leading CR/LF between pipelined requests tolerated). First byte to end of head must arrive within `header_read_timeout` (`Error::Timeout("reading the request head")`); an idle connection with nothing buffered ends quietly (`Ok`) after `idle_timeout` or when `shutdown` fires. A head larger than `limits.max_head_bytes` is `HeadersTooLarge`.
2. Parse and frame the request. Any `ProtoError` is answered with a minimal bodiless response of its `status()` (best effort, `Connection: close`) and returned as `Err(Error::Proto(..))`; the handler never sees the request.
3. `Expect`: a value other than `100-continue` is answered `417` and the connection closes; `100-continue` on an HTTP/1.1 request with a body sends `HTTP/1.1 100 Continue` before the handler runs.
4. Run `service.call(request)`. The request `Body` streams from the wire through a channel of capacity 1 (backpressure). If the handler drops the body, the remainder is read and discarded up to `drain_limit` bytes within `drain_timeout`; past that the response carries `Connection: close`.
5. Write the response (see [Internals](internals.md#response-framing-by-the-server)); keep going only if the request wanted keep-alive, its body was fully read cleanly and `shutdown` is not set.
6. A response with `on_upgrade` set: after the head is written, the buffered input and the stream are handed to the hook as `Upgraded`, and the function returns `Ok(())`.

Return value: `Ok(())` when the peer closes between requests, on idle timeout, on shutdown, after `Connection: close` or upgrade; `Err` for I/O errors, protocol errors, `Timeout` (head read, write), `UnexpectedEof`, `BodyLength`, `Body`.

## `h1::client` (feature `client`)

```rust
#[derive(Debug, Clone)]
pub struct ClientConfig { pub limits: Limits, pub response_head_timeout: Duration, pub write_timeout: Duration }
impl Default for ClientConfig;   // see Configuration

pub struct ClientConn<I: Io> { /* Reader<ReadHalf<I>>, WriteHalf<I> */ }   // Debug
impl<I: Io> ClientConn<I> {
    pub fn new(io: I) -> Self;
    pub fn is_reusable(&mut self) -> bool;
}

pub struct Exchange<I: Io> { pub response: Response, pub reuse: oneshot::Receiver<ClientConn<I>> }   // Debug

pub async fn send_request<I: Io>(conn: ClientConn<I>, request: Request, config: &ClientConfig) -> Result<Exchange<I>, Error>;
```

* `ClientConn::is_reusable`: polls the idle socket once without blocking; false if the peer closed it or sent bytes nobody asked for. Use it before reusing a pooled connection.
* `send_request`: consumes the connection for one exchange. It sets `Host` from the URI authority (without userinfo) when the request has no `Host`; writes `METHOD path-and-query HTTP/1.1` (always HTTP/1.1); framing: a declared `Content-Length` header wins, else a known body length is sent with `Content-Length` (not added for an empty body unless the method is `POST`, `PUT` or `PATCH`), else `Transfer-Encoding: chunked`. A body that writes more or fewer bytes than declared is `Error::BodyLength`. Then it waits up to `response_head_timeout` for the head (`Error::Timeout("waiting for the response head")`), skipping `1xx` interim responses. `Request.version`, `.extensions` and `.uri` scheme are not used on the wire; the client does not add `Connection`, `User-Agent` or `Accept-Encoding`.
* `Exchange.response.body` streams the response body (sized or chunked or until close). `Exchange.reuse` yields the `ClientConn` back when the body was read to its end and `wants_keep_alive` allows reuse and the framing was not `UntilClose`; if the body is dropped early or an error occurs the sender is dropped, so `reuse.await` errors and the connection must be discarded. A response with no body is returned with its connection already available in `reuse`.
* There is no timeout on reading the response body; wrap reads in your own deadline.
