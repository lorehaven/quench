---
title: API reference - core types
parent: quench-proto
nav_order: 2
---

Message types and helpers, in `lib.rs` order. Re-exports at the crate root: `Body`, `BoxError`, `Error`, `Extensions`, `HeaderMap`, `HeaderName`, `HeaderValue`, `Request`, `Response`, `Method`, `StatusCode`, `Uri`, `Version`, and (feature `tls`) `quench_proto::tls` = `quench_tls`. Other modules: [HTTP/1.1 codec and drivers](api-h1.md), [cookie, multipart, sse, ws](api-extras.md). `Bytes` is `quench_core::bytes::Bytes`. All types here are `Send + Sync` unless noted.

## `body`

```rust
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub struct Body(/* private */);     // Default = empty; Debug; Stream<Item = Result<Bytes, BoxError>>
impl Body {
    #[must_use] pub const fn empty() -> Self;
    #[must_use] pub fn from_bytes(bytes: impl Into<Bytes>) -> Self;
    pub fn from_stream<S, E>(stream: S) -> Self
        where S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static, E: Into<BoxError> + 'static;
    pub fn from_stream_sized<S, E>(stream: S, len: u64) -> Self /* same bounds */;
    #[must_use] pub fn exact_len(&self) -> Option<u64>;
    #[must_use] pub const fn is_empty_body(&self) -> bool;
    pub async fn collect_limited(self, limit: usize) -> Result<Bytes, CollectError>;
}
impl From<Bytes> for Body; impl From<Vec<u8>> for Body; impl From<String> for Body; impl From<&'static str> for Body;

#[derive(Debug)]
pub enum CollectError { TooLarge(usize), Read(BoxError) }   // Display, std::error::Error
```

* `from_bytes`: a fixed body; the writer sends `Content-Length`. `from_stream`: unknown length; sent chunked on HTTP/1.1. `from_stream_sized`: a stream with a declared exact length (sent with `Content-Length`; the driver fails with `Error::BodyLength` if it differs).
* `exact_len`: `Some` for fixed and sized bodies. `is_empty_body`: true for `Body::empty()`.
* `collect_limited(limit)`: when the exact length is known and already above `limit` it fails at once without reading; otherwise reads chunks and fails with `CollectError::TooLarge(limit)` as soon as the total would pass `limit`. Stream errors become `CollectError::Read`.

## `cookie`, `multipart`, `sse`, `ws`

See [api-extras.md](api-extras.md).

## `date`

```rust
#[must_use] pub fn format(time: &Time) -> String;     // "Sun, 06 Nov 1994 08:49:37 GMT"
#[must_use] pub fn now() -> String;
#[must_use] pub fn parse(text: &str) -> Option<Time>;
```

`Time` is `quench_core::time::Time`. `parse` accepts IMF-fixdate only (`Day, DD Mon YYYY HH:MM:SS GMT`); the obsolete RFC 850 and asctime forms return `None`. Month names are matched case-insensitively; the weekday is not checked.

## `error`

```rust
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Proto(h1::ProtoError),      // the peer broke the HTTP/1.1 rules
    Timeout(&'static str),      // a deadline passed; the text says which step
    UnexpectedEof,              // peer closed mid-message
    BodyLength,                 // body did not match its Content-Length
    Body(BoxError),             // a body stream failed while being written
}
impl From<std::io::Error> for Error; impl From<h1::ProtoError> for Error;
impl std::error::Error for Error { fn source(&self) -> .. }   // Io, Proto, Body have sources
```

Display texts: `i/o error: {e}`; the `ProtoError` text; `timed out {what}`; `connection closed mid-message`; `body length does not match its Content-Length`; `body stream failed: {e}`. `Timeout` texts produced by the drivers: `reading the request head`, `writing` (server and client), and the client's response-head wait (see [Errors](errors.md)).

## `extensions`

```rust
#[derive(Default)] pub struct Extensions { /* HashMap<TypeId, Box<dyn Any + Send + Sync>> */ }
impl Extensions {
    #[must_use] pub fn new() -> Self;
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T>;
    #[must_use] pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T>;
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T>;
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T>;
    #[must_use] pub fn len(&self) -> usize;
    #[must_use] pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);
}
```

A type-keyed map: at most one value per type; `insert` returns the previous value of that type. `Debug` prints the count only. Not `Clone`.

## `header`

```rust
#[must_use] pub const fn is_token_byte(b: u8) -> bool;     // RFC 9110 5.6.2 tchar

#[derive(Debug, Clone, PartialEq, Eq)] pub struct InvalidHeaderName;    // "invalid HTTP header name"
#[derive(Debug, Clone, PartialEq, Eq)] pub struct InvalidHeaderValue;   // "failed to parse header value"
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ToStrError;           // "header value is not visible ASCII"

#[derive(Clone)] pub struct HeaderName(/* private */);       // stored lowercase; Eq/Hash/Ord/Display/Debug/AsRef<str>/FromStr/TryFrom<&str>; PartialEq<str>, PartialEq<&str>
impl HeaderName {
    #[must_use] pub fn from_static(name: &'static str) -> Self;                       // panics: empty, non-token byte or uppercase
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderName>;                 // folds to lowercase
    #[must_use] pub fn as_str(&self) -> &str;
}

#[derive(Clone, PartialEq, Eq, Hash)] pub struct HeaderValue(Bytes);   // PartialEq<str>, PartialEq<&str>, AsRef<[u8]>, FromStr, TryFrom<&str|String>
impl HeaderValue {
    #[must_use] pub fn from_static(src: &'static str) -> Self;                        // panics on a disallowed byte
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderValue>;
    pub fn from_str(src: &str) -> Result<Self, InvalidHeaderValue>;
    pub fn to_str(&self) -> Result<&str, ToStrError>;
    #[must_use] pub fn as_bytes(&self) -> &[u8];
    #[must_use] pub const fn len(&self) -> usize;
    #[must_use] pub const fn is_empty(&self) -> bool;
}
impl From<u16|u32|u64|usize|i32|i64> for HeaderValue;     // decimal text
```

* A header **value** may contain `\t` and bytes `0x20..=0xFF` except `0x7F`; any other control byte (including CR and LF) is rejected. This is what prevents response splitting. `to_str` fails for values with non-ASCII bytes. `Debug` prints the value as an escaped string.
* The well-known names are `pub const` of type `HeaderName` in `quench_proto::header`: `ACCEPT, ACCEPT_CHARSET, ACCEPT_ENCODING, ACCEPT_LANGUAGE, ACCEPT_RANGES, ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE, ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, AGE, ALLOW, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LANGUAGE, CONTENT_LENGTH, CONTENT_LOCATION, CONTENT_RANGE, CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE, DATE, ETAG, EXPECT, EXPIRES, FORWARDED, FROM, HOST, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE, IF_UNMODIFIED_SINCE, LAST_MODIFIED, LINK, LOCATION, ORIGIN, PRAGMA, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, RANGE, REFERER, REFERRER_POLICY, RETRY_AFTER, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION, SERVER, SET_COOKIE, STRICT_TRANSPORT_SECURITY, TE, TRAILER, TRANSFER_ENCODING, UPGRADE, USER_AGENT, VARY, VIA, WWW_AUTHENTICATE, X_CONTENT_TYPE_OPTIONS, X_FORWARDED_FOR, X_FORWARDED_HOST, X_FORWARDED_PROTO, X_FRAME_OPTIONS, X_REQUEST_ID` (71 constants; the value is the kebab-case lowercase name, e.g. `CONTENT_TYPE` is `content-type`).

```rust
pub trait AsHeaderName { /* hidden method */ }   // implemented for str, &str, String, HeaderName, &HeaderName; matches case-insensitively
pub trait IntoHeaderName { /* hidden method */ } // implemented for HeaderName, &HeaderName, &str (parsed; an invalid name is ignored by insert/append)

#[derive(Clone, Default, PartialEq, Eq)]
pub struct HeaderMap { /* Vec<(HeaderName, HeaderValue)> */ }
impl HeaderMap {
    #[must_use] pub const fn new() -> Self;
    #[must_use] pub fn with_capacity(capacity: usize) -> Self;
    #[must_use] pub const fn len(&self) -> usize;                   // header lines; a name with two values counts twice
    #[must_use] pub const fn is_empty(&self) -> bool;
    #[must_use] pub fn get<K: AsHeaderName + ?Sized>(&self, name: &K) -> Option<&HeaderValue>;
    pub fn get_all<'a, K: AsHeaderName + ?Sized>(&'a self, name: &'a K) -> impl Iterator<Item = &'a HeaderValue>;
    #[must_use] pub fn contains_key<K: AsHeaderName + ?Sized>(&self, name: &K) -> bool;
    pub fn insert<K: IntoHeaderName>(&mut self, name: K, value: HeaderValue) -> Option<HeaderValue>;
    pub fn append<K: IntoHeaderName>(&mut self, name: K, value: HeaderValue) -> bool;
    pub fn remove<K: AsHeaderName + ?Sized>(&mut self, name: &K) -> Option<HeaderValue>;
    pub fn clear(&mut self);
    pub fn iter(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)>;
    pub fn keys(&self) -> impl Iterator<Item = &HeaderName>;
    pub fn values(&self) -> impl Iterator<Item = &HeaderValue>;
    #[must_use] pub fn get_joined<K: AsHeaderName + ?Sized>(&self, name: &K) -> Option<Cow<'_, str>>;
}
// IntoIterator for &HeaderMap and HeaderMap; FromIterator / Extend of (HeaderName, HeaderValue); Debug
```

* An ordered multimap with linear lookups. `get`: first value in insertion order. `insert`: replaces every value of that name (the new one keeps the first old entry's position; the first old value is returned). `append`: adds after existing values; returns whether the name was present (use for `Set-Cookie`). `remove`: removes all values and returns the first. `keys`: distinct names in first-seen order. `get_joined`: values joined with `, `; a single value is borrowed; values that are not visible ASCII (`to_str` fails) are skipped. `append` with an invalid `&str` name does nothing and returns `false`; `insert` with one returns `None` and does nothing.

## `io`

```rust
pub trait Io: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static {}   // blanket impl for every such type
pub struct Upgraded { /* Box<dyn Io>, prefix: Bytes */ }   // AsyncRead + AsyncWrite, Debug
pub struct OnUpgrade(/* private callback */);              // Debug; constructed by Response::on_upgrade
```

`Upgraded` is the raw connection after a `101` response: reads return the bytes the peer already sent past the head (the *prefix*) first, then the live stream. Writes go straight to the stream. `OnUpgrade` has no public constructor or methods.

## `message`

```rust
#[derive(Debug, Default)]
pub struct Request {
    pub method: Method, pub uri: Uri, pub version: Version,
    pub headers: HeaderMap, pub extensions: Extensions, pub body: Body,
}
impl Request {
    #[must_use] pub fn new(method: Method, uri: Uri) -> Self;                    // version HTTP/1.1, no headers, empty body
    #[must_use] pub fn header(self, name: &str, value: &str) -> Self;            // replaces; invalid names/values are silently dropped
    #[must_use] pub fn body(self, body: impl Into<Body>) -> Self;
}

#[derive(Debug, Default)]
pub struct Response {
    pub status: StatusCode, pub version: Version,
    pub headers: HeaderMap, pub extensions: Extensions, pub body: Body,
    /* pub(crate) upgrade */
}
impl Response {
    #[must_use] pub fn new(status: StatusCode) -> Self;
    #[must_use] pub fn header(self, name: &str, value: &str) -> Self;
    #[must_use] pub fn body(self, body: impl Into<Body>) -> Self;
    #[must_use] pub fn on_upgrade<F, Fut>(self, handler: F) -> Self
        where F: FnOnce(io::Upgraded) -> Fut + Send + 'static, Fut: Future<Output = ()> + Send + 'static;
    #[must_use] pub const fn is_upgrade(&self) -> bool;
}
```

`Request::default()` is `GET /` (Method and Uri defaults), HTTP/1.1; `Response::default()` is `200`, HTTP/1.1. `on_upgrade`: after the head is written (normally `101`), the driver hands the raw stream to `handler` and ends the connection loop; the body is ignored, `Date` and `Connection` headers are not added.

## `method`

```rust
#[derive(Clone, PartialEq, Eq, Hash)] pub struct Method(/* private */);
#[derive(Debug, Clone, PartialEq, Eq)] pub struct InvalidMethod;    // "invalid HTTP method"
impl Method {
    pub const GET: Self; HEAD; POST; PUT; DELETE; CONNECT; OPTIONS; TRACE; PATCH   // nine consts
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidMethod>;
    #[must_use] pub fn as_str(&self) -> &str;
    #[must_use] pub const fn is_safe(&self) -> bool;          // GET, HEAD, OPTIONS, TRACE
    #[must_use] pub const fn is_idempotent(&self) -> bool;    // safe methods plus PUT and DELETE
}
// Default (GET), Display, Debug, FromStr, TryFrom<&str>, AsRef<str>, PartialEq<str>, PartialEq<&str>, PartialEq<Method> for &Method, PartialEq<&Self>
```

Methods are case-sensitive tokens; any other valid token (`PROPFIND`) becomes an extension method (`is_safe` and `is_idempotent` are `false` for them).

## `status`

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)] pub struct StatusCode(u16);
#[derive(Debug, Clone, PartialEq, Eq)] pub struct InvalidStatusCode;   // "invalid status code"
impl StatusCode {
    pub const CONTINUE .. NETWORK_AUTHENTICATION_REQUIRED: Self;   // 62 named constants (list below)
    pub const fn from_u16(code: u16) -> Result<Self, InvalidStatusCode>;       // 100..=999
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidStatusCode>;           // exactly three ASCII digits
    #[must_use] pub const fn as_u16(self) -> u16;
    #[must_use] pub const fn is_informational(self) -> bool;   // 1xx
    #[must_use] pub const fn is_success(self) -> bool;         // 2xx
    #[must_use] pub const fn is_redirection(self) -> bool;     // 3xx
    #[must_use] pub const fn is_client_error(self) -> bool;    // 4xx
    #[must_use] pub const fn is_server_error(self) -> bool;    // 5xx
    #[must_use] pub const fn canonical_reason(self) -> Option<&'static str>;
    #[must_use] pub const fn has_no_body(self) -> bool;        // 1xx, 204, 304
}
// Default (200), Display ("200 OK"), Debug, From<StatusCode> for u16, TryFrom<u16>, PartialEq<u16>
```

Constants: `CONTINUE 100, SWITCHING_PROTOCOLS 101, PROCESSING 102, EARLY_HINTS 103, OK 200, CREATED 201, ACCEPTED 202, NON_AUTHORITATIVE_INFORMATION 203, NO_CONTENT 204, RESET_CONTENT 205, PARTIAL_CONTENT 206, MULTI_STATUS 207, ALREADY_REPORTED 208, IM_USED 226, MULTIPLE_CHOICES 300, MOVED_PERMANENTLY 301, FOUND 302, SEE_OTHER 303, NOT_MODIFIED 304, USE_PROXY 305, TEMPORARY_REDIRECT 307, PERMANENT_REDIRECT 308, BAD_REQUEST 400, UNAUTHORIZED 401, PAYMENT_REQUIRED 402, FORBIDDEN 403, NOT_FOUND 404, METHOD_NOT_ALLOWED 405, NOT_ACCEPTABLE 406, PROXY_AUTHENTICATION_REQUIRED 407, REQUEST_TIMEOUT 408, CONFLICT 409, GONE 410, LENGTH_REQUIRED 411, PRECONDITION_FAILED 412, PAYLOAD_TOO_LARGE 413, URI_TOO_LONG 414, UNSUPPORTED_MEDIA_TYPE 415, RANGE_NOT_SATISFIABLE 416, EXPECTATION_FAILED 417, IM_A_TEAPOT 418, MISDIRECTED_REQUEST 421, UNPROCESSABLE_ENTITY 422, LOCKED 423, FAILED_DEPENDENCY 424, TOO_EARLY 425, UPGRADE_REQUIRED 426, PRECONDITION_REQUIRED 428, TOO_MANY_REQUESTS 429, REQUEST_HEADER_FIELDS_TOO_LARGE 431, UNAVAILABLE_FOR_LEGAL_REASONS 451, INTERNAL_SERVER_ERROR 500, NOT_IMPLEMENTED 501, BAD_GATEWAY 502, SERVICE_UNAVAILABLE 503, GATEWAY_TIMEOUT 504, HTTP_VERSION_NOT_SUPPORTED 505, VARIANT_ALSO_NEGOTIATES 506, INSUFFICIENT_STORAGE 507, LOOP_DETECTED 508, NOT_EXTENDED 510, NETWORK_AUTHENTICATION_REQUIRED 511`. `canonical_reason` returns the registered phrase for these (`None` for others such as `299`); `Display` is the number and the phrase (`200 OK`), or `<unknown status code>` after the number for unregistered codes.

## `uri`

```rust
#[derive(Clone, PartialEq, Eq, Hash)] pub struct Uri { /* raw: String + offsets */ }   // Display/Debug print the text; Default; FromStr; TryFrom<&str|String>
#[derive(Debug, Clone, PartialEq, Eq)] pub enum InvalidUri { Empty, InvalidChar, InvalidScheme, InvalidAuthority, InvalidPort }
impl Uri {
    pub fn parse(src: &str) -> Result<Self, InvalidUri>;
    #[must_use] pub fn scheme(&self) -> Option<&str>;
    #[must_use] pub fn authority(&self) -> Option<&str>;
    #[must_use] pub fn host(&self) -> Option<&str>;
    #[must_use] pub fn port_u16(&self) -> Option<u16>;
    #[must_use] pub fn path(&self) -> &str;
    #[must_use] pub fn query(&self) -> Option<&str>;
    #[must_use] pub fn path_and_query(&self) -> &str;
    #[must_use] pub fn as_str(&self) -> &str;
    pub fn resolve(&self, reference: &str) -> Result<Self, InvalidUri>;
}
```

* `parse` accepts: `*`; origin form (starts with `/`); absolute form `scheme://authority[/path][?query]` (an absolute URL with no path gets `/` inserted); authority form `host:port` (for `CONNECT`). It rejects: empty text, any byte `<= 0x20`, `>= 0x7F` or `#` (`InvalidChar`: fragments, spaces, controls, non-ASCII), a scheme that does not match `[A-Za-z][A-Za-z0-9+.-]*` (`InvalidScheme`), an empty or malformed authority, `@` in authority form, brackets without closing, or forbidden characters in the host (`InvalidAuthority`), a non-numeric or out-of-range port (`InvalidPort`). The scheme is not restricted to `http`/`https` (despite the doc comment on `scheme()`).
* `authority()`: `userinfo@host:port` as written. `host()`: without userinfo and port; IPv6 literals keep their brackets. `port_u16()`: the explicit port only. `path()`: `/` for an absolute URL without path; empty for authority form. `path_and_query()`: what goes in an origin-form request line. `query()`: text after the first `?`, without it.
* `resolve(reference)`: strips a fragment from `reference`; absolute references replace; `//host/path` keeps the scheme; `/path` keeps the authority (dot segments collapsed); `?query` keeps the path; a relative path replaces the last segment (dot segments collapsed). Defaults the scheme to `http` and authority to empty when `self` is not absolute. Errors when the result does not parse.
