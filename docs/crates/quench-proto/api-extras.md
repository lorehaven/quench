---
title: API reference - cookie, multipart, sse, ws
parent: quench-proto
nav_order: 3
---

## `cookie`

RFC 6265. Attribute order written by `Display`: `HttpOnly; SameSite; Secure; Path; Domain; Max-Age; Expires`. Parsing is lenient like a browser's jar: unknown attributes and malformed values for known ones are ignored; only a missing `=` or an empty name fails. `Time` and `Duration` are `quench_core::time` types.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum SameSite { Strict, Lax, None }       // Display: "Strict" "Lax" "None"
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ParseError { MissingPair, EmptyName }  // Display, std::error::Error

#[derive(Debug, Clone, PartialEq, Eq)] pub struct Cookie { /* name, value, path, domain, max_age, expires, secure, http_only, same_site */ }
impl Cookie {
    #[must_use] pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self;
    #[must_use] pub fn build(pair: impl Into<Self>) -> CookieBuilder;
    pub fn parse(text: &str) -> Result<Self, ParseError>;
    #[must_use] pub fn name(&self) -> &str;
    #[must_use] pub fn value(&self) -> &str;
    #[must_use] pub fn path(&self) -> Option<&str>;
    #[must_use] pub fn domain(&self) -> Option<&str>;
    #[must_use] pub const fn max_age(&self) -> Option<Duration>;
    #[must_use] pub const fn expires(&self) -> Option<Time>;
    #[must_use] pub const fn secure(&self) -> Option<bool>;
    #[must_use] pub const fn http_only(&self) -> Option<bool>;
    #[must_use] pub const fn same_site(&self) -> Option<SameSite>;
}
// Display (Set-Cookie value), FromStr (= parse), From<(N, V)> where N, V: Into<String>, From<CookieBuilder>

#[derive(Debug, Clone, PartialEq, Eq)] pub struct CookieBuilder(Cookie);       // Display
impl CookieBuilder {
    #[must_use] pub fn path(self, path: impl Into<String>) -> Self;
    #[must_use] pub fn domain(self, domain: impl Into<String>) -> Self;
    #[must_use] pub const fn max_age(self, max_age: Duration) -> Self;
    #[must_use] pub const fn expires(self, expires: Time) -> Self;
    #[must_use] pub const fn secure(self, secure: bool) -> Self;
    #[must_use] pub const fn http_only(self, http_only: bool) -> Self;
    #[must_use] pub const fn same_site(self, same_site: SameSite) -> Self;
    #[must_use] pub fn build(self) -> Cookie;
}

pub fn parse_cookie_header(header: &str) -> impl Iterator<Item = (&str, &str)>;
#[must_use] pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str>;
```

* `Cookie::parse(text)`: the first `;` segment must be `name=value` (name trimmed and non-empty, value trimmed; value may contain `=`). Attributes (names case-insensitive): `Secure`, `HttpOnly`, `Max-Age` (digits; a leading `-` means 0 seconds; other text ignored), `Domain` (a leading dot is dropped; an empty result ignored), `Path`, `SameSite` (`Strict`/`Lax`/`None`, case-insensitive; other text ignored), `Expires` (IMF-fixdate only; else ignored).
* `Display`: `name=value` then, in order: `; HttpOnly` if set true; `; SameSite=X`; `; Secure` if set true **or** if `secure` was never set and `SameSite` is `None`; `; Path=..`; `; Domain=..`; `; Max-Age=<seconds>`; `; Expires=<IMF-fixdate>`. No percent-encoding or validation of names and values.
* `parse_cookie_header(header)`: the `name=value` pairs of a request `Cookie` header, split on `;`, trimmed, skipping segments without `=` or with an empty name. `cookie_value` returns the first pair with that exact (case-sensitive) name.

## `multipart` (feature `multipart`)

Streaming `multipart/form-data` reader (RFC 7578 over RFC 2046). Field bodies are streamed; only a delimiter-length tail is held back while searching for the next boundary.

```rust
#[derive(Debug, Clone, Copy)]
pub struct Limits { pub whole_stream: Option<u64>, pub per_field: Option<u64>, pub max_header_bytes: usize, pub max_fields: usize }
impl Default for Limits;   // None, None, 16 * 1024, 1024

#[derive(Debug)]
pub enum Error {
    MissingBoundary, InvalidBoundary, Truncated, Malformed(&'static str), HeadersTooLarge, TooManyFields,
    StreamTooLarge(u64), FieldTooLarge(u64), NotUtf8, Stream(BoxError),
}   // Display, std::error::Error (source for Stream)

pub fn parse_boundary(content_type: &str) -> Result<String, Error>;

pub struct Multipart<S> { /* stream, delimiter, buf, state, limits, ... */ }     // Debug
impl<S, E> Multipart<S> where S: Stream<Item = Result<Bytes, E>> + Unpin, E: Into<BoxError> {
    pub fn new(stream: S, boundary: &str, limits: Limits) -> Self;
    pub async fn next_field(&mut self) -> Result<Option<Field<'_, S>>, Error>;
}

pub struct Field<'a, S> { /* &'a mut Multipart<S>, name, file_name, content_type, headers, read */ }   // Debug
impl<S, E> Field<'_, S> where /* same bounds */ {
    #[must_use] pub fn name(&self) -> Option<&str>;
    #[must_use] pub fn file_name(&self) -> Option<&str>;
    #[must_use] pub fn content_type(&self) -> Option<&str>;
    #[must_use] pub const fn headers(&self) -> &HeaderMap;
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, Error>;
    pub async fn bytes(self) -> Result<Bytes, Error>;
    pub async fn text(self) -> Result<String, Error>;
}
```

* `parse_boundary(content_type)`: `MissingBoundary` unless the type is `multipart/form-data` with a `boundary` parameter; `InvalidBoundary` if empty, longer than 70 bytes or containing characters RFC 2046 does not allow. Quoted parameter values are supported.
* `Limits`: `whole_stream` caps all bytes read (boundaries and headers included; `StreamTooLarge(limit)`); `per_field` caps one field body (`FieldTooLarge(limit)`); `max_header_bytes` caps one part's header block (`HeadersTooLarge`); `max_fields` caps the number of parts (`TooManyFields`).
* `next_field`: the next part, or `None` after the closing delimiter; an unread previous field is skipped. After any `Err` drop the reader. `Field::name` and `file_name` come from `Content-Disposition` (quoted names, RFC 5987 `filename*=UTF-8''...` handled); `headers` are the part's headers. `chunk` yields body pieces; `bytes`/`text` buffer the whole body (`text`: `NotUtf8` for invalid UTF-8). Preamble and epilogue are skipped; transport padding after a delimiter is accepted.
* Display texts: `missing or invalid multipart boundary`, `invalid multipart boundary`, `multipart body ended before its closing boundary`, `malformed multipart body: {what}`, `multipart part headers too large`, `too many multipart fields`, `multipart body exceeds {n} bytes`, `multipart field exceeds {n} bytes`, `multipart text field is not valid UTF-8`, `failed to read multipart body: {e}`.

## `sse` (feature `sse`)

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)] pub struct Event { /* name, data: Vec<String>, id, retry, comment */ }
impl Event {
    #[must_use] pub fn new() -> Self;
    #[must_use] pub fn data(self, data: impl AsRef<str>) -> Self;
    #[must_use] pub fn event(self, name: impl AsRef<str>) -> Self;
    #[must_use] pub fn id(self, id: impl AsRef<str>) -> Self;
    #[must_use] pub const fn retry(self, millis: u64) -> Self;
    #[must_use] pub fn comment(self, text: impl AsRef<str>) -> Self;
    #[must_use] pub fn to_bytes(&self) -> Bytes;
}
// Display = the wire form
pub fn response<S>(events: S) -> Response where S: Stream<Item = Event> + Unpin + Send + Sync + 'static;
```

* `data`: each line of the text (split on `\n`, `\r\n`, `\r`) becomes one `data:` line; calling it again replaces the earlier data. `event` and `id`: newline characters are removed. `comment`: a `:` line; an empty comment is the keep-alive line `:`.
* Wire form, stable order: `: comment`, `event: name`, `id: x`, `retry: n`, `data: line` (one per line), then a blank line. Example: `: hi\nevent: tick\nid: 7\nretry: 1500\ndata: a\ndata: b\ndata: c\n\n`.
* `response`: `200` with `content-type: text/event-stream`, `cache-control: no-cache`, `x-accel-buffering: no` and a chunked streaming body of `event.to_bytes()`.

## `ws` (feature `ws`)

RFC 6455, no extensions (any reserved bit is a protocol error). Client frames must be masked, server frames must not.

```rust
pub const HANDSHAKE_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
#[must_use] pub fn accept_key(client_key: &str) -> String;     // base64(SHA-1(key + GUID))
#[must_use] pub fn generate_key() -> String;                   // base64 of 16 random bytes

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError { NotGet, MissingUpgrade, MissingConnection, MissingKey, BadKey, UnsupportedVersion }  // Display, std::error::Error
pub fn validate_upgrade(method: &Method, headers: &HeaderMap) -> Result<String, HandshakeError>;

#[derive(Debug, Clone, Copy)] pub struct Config { pub max_message_size: usize, pub max_frame_size: usize }
impl Default for Config;   // 16 * 1024 * 1024 both

pub fn accept<F, Fut>(method: &Method, headers: &HeaderMap, config: Config, handler: F) -> Result<Response, HandshakeError>
    where F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static, Fut: Future<Output = ()> + Send + 'static;
pub fn switch<F, Fut>(key: &str, config: Config, handler: F) -> Response /* same bounds */;

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Opcode { Continuation, Text, Binary, Close, Ping, Pong }
impl Opcode { #[must_use] pub const fn is_control(self) -> bool; }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Frame { pub fin: bool, pub opcode: Opcode, pub payload: Vec<u8> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError { ReservedBits, UnknownOpcode, FragmentedControlFrame, ControlFrameTooLong, BadMasking,
    UnexpectedContinuation, InterruptedMessage, NonMinimalLength, InvalidClosePayload, InvalidCloseCode, InvalidUtf8, TooBig }
impl ProtocolError { #[must_use] pub const fn close_code(self) -> u16; }

pub fn encode_frame(out: &mut Vec<u8>, fin: bool, opcode: Opcode, payload: &[u8], mask: Option<[u8; 4]>);
pub fn decode_frame(buf: &[u8], expect_masked: bool, max_payload: usize) -> Result<Option<(Frame, usize)>, ProtocolError>;

#[derive(Debug, Clone, PartialEq, Eq)] pub struct CloseFrame { pub code: u16, pub reason: String }
#[derive(Debug, Clone, PartialEq, Eq)] pub enum Message { Text(String), Binary(Vec<u8>), Ping(Vec<u8>), Pong(Vec<u8>), Close(Option<CloseFrame>) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Role { Server, Client }

#[derive(Debug)] pub enum Error { Io(std::io::Error), Protocol(ProtocolError), Closed, ControlTooLong }   // Display, std::error::Error, From<io::Error>

pub struct WebSocket<I> { /* io, role, config, inbox, partial, close_sent, closed */ }     // Debug
impl<I: AsyncRead + AsyncWrite + Unpin> WebSocket<I> {
    pub const fn new(io: I, role: Role, config: Config) -> Self;
    #[must_use] pub const fn is_closed(&self) -> bool;
    pub async fn send(&mut self, message: Message) -> Result<(), Error>;
    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), Error>;
    pub async fn recv(&mut self) -> Result<Option<Message>, Error>;
}
pub const fn client<I: Io>(io: I, config: Config) -> WebSocket<I>;
#[must_use] pub fn client_handshake_headers(key: &str) -> Vec<(&'static str, HeaderValue)>;
```

* `validate_upgrade`: Checked in this order: `NotGet` unless GET; the `Upgrade` header must list the token `websocket` (`MissingUpgrade`); `Connection` must list `upgrade` (`MissingConnection`; tokens are comma-separated and case-insensitive); `Sec-WebSocket-Version` must be `13` (`UnsupportedVersion`); `Sec-WebSocket-Key` must be present (`MissingKey`) and be standard base64 of exactly 16 bytes (`BadKey`). Returns the trimmed key. Display texts: `WebSocket upgrades must be GET requests`, `missing `Upgrade: websocket``, `missing `Connection: Upgrade``, `missing Sec-WebSocket-Key`, `Sec-WebSocket-Key is not a 16-byte base64 value`, `only Sec-WebSocket-Version 13 is supported`.
* `accept` = `validate_upgrade` + `switch`. `switch` returns `101 Switching Protocols` with `Upgrade: websocket`, `Connection: Upgrade`, `Sec-WebSocket-Accept`, and runs `handler` with a `WebSocket<Upgraded>` (role `Server`) once the head is written. Subprotocols are not negotiated.
* `encode_frame`: header `FIN|opcode`, 7-bit / 16-bit / 64-bit length (minimal form), optional 4-byte mask and masked payload. `decode_frame`: `Ok(None)` when `buf` holds only part of a frame; errors as early as the header allows (`ReservedBits`, `UnknownOpcode`, `BadMasking`, `NonMinimalLength` for a 16-bit length under 126 or a 64-bit length under 65 536 or with the top bit set, `FragmentedControlFrame`, `ControlFrameTooLong` (> 125), `TooBig` when the declared length exceeds `max_payload`, before the payload is read or allocated). The returned frame is unmasked.
* `close_code`: `1007` for `InvalidUtf8` and `InvalidClosePayload`, `1009` for `TooBig`, `1002` for all others. Display texts are in [Errors](errors.md#websocket).
* `WebSocket::send`: `Text`/`Binary` as one unfragmented frame; `Ping`/`Pong` payloads over 125 bytes are `ControlTooLong`; sending after a close was sent is `Closed`. `close(code, reason)`: sends a Close frame (reason truncated to 123 bytes at a character boundary) and does nothing, returning `Ok`, if a close was already sent. `send(Message::Close(frame))` sends a close with the given frame (`None` = empty payload; a payload over 125 bytes is `ControlTooLong`). `recv`: pings are answered with pongs (unless a close was sent) and not returned; `Pong` frames are returned as `Message::Pong`; fragmented messages are reassembled (limit `max_message_size`; single frame limit `max_frame_size`); text is UTF-8 validated; a peer Close is echoed (same code, empty reason) and returned once as `Message::Close(..)`, afterwards `Ok(None)`; the stream ending with no data pending is `Ok(None)`, with a partial message or partial frame pending it is `Err(Error::Io(UnexpectedEof))`. Valid close codes: 1000-1003, 1007-1011, 3000-4999 (an empty payload is allowed; one byte is not).
* `client(io, config)`: wraps an already-handshaken stream as a client (frames masked with random keys). `client_handshake_headers(key)`: `upgrade: websocket`, `connection: Upgrade`, `sec-websocket-version: 13`, `sec-websocket-key: <key>`.
