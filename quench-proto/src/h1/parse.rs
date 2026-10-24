//! Parsing and serialising HTTP/1.x message heads (RFC 9112), strictly.
//!
//! Anything that could make two parsers disagree about where a message ends is
//! rejected rather than interpreted: obs-fold, whitespace before the colon, a
//! bare LF, conflicting or malformed `Content-Length`, and a message carrying
//! both `Content-Length` and `Transfer-Encoding`.

use crate::header::{HeaderMap, HeaderName, HeaderValue};
use crate::method::Method;
use crate::status::StatusCode;
use crate::uri::Uri;
use crate::version::Version;
use std::fmt;

/// Size caps applied while reading a head.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The whole head: start line plus headers plus the blank line.
    pub max_head_bytes: usize,
    pub max_headers: usize,
    /// The request target.
    pub max_uri_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_head_bytes: 64 * 1024,
            max_headers: 100,
            max_uri_bytes: 8 * 1024,
        }
    }
}

/// A protocol violation, with the status a server should answer it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoError {
    /// Malformed syntax; the text says what.
    BadRequest(&'static str),
    UriTooLong,
    HeadersTooLarge,
    VersionNotSupported,
    /// A transfer coding other than `chunked`.
    NotImplemented,
}

impl ProtoError {
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::UriTooLong => StatusCode::URI_TOO_LONG,
            Self::HeadersTooLarge => StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            Self::VersionNotSupported => StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
        }
    }
}

impl fmt::Display for ProtoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(why) => write!(f, "malformed HTTP message: {why}"),
            Self::UriTooLong => f.write_str("request target too long"),
            Self::HeadersTooLarge => f.write_str("message head too large"),
            Self::VersionNotSupported => f.write_str("HTTP version not supported"),
            Self::NotImplemented => f.write_str("transfer coding not implemented"),
        }
    }
}

impl std::error::Error for ProtoError {}

/// How a message body is delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framing {
    /// No body.
    None,
    Length(u64),
    Chunked,
    /// A response body that ends when the connection closes.
    UntilClose,
}

#[derive(Debug)]
pub struct RequestHead {
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub headers: HeaderMap,
}

#[derive(Debug)]
pub struct ResponseHead {
    pub status: StatusCode,
    pub version: Version,
    pub headers: HeaderMap,
}

/// The index just past the blank line that ends a head, if `buf` holds a whole one.
#[must_use]
pub fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn parse_version(text: &[u8]) -> Result<Version, ProtoError> {
    match text {
        b"HTTP/1.1" => Ok(Version::HTTP_11),
        b"HTTP/1.0" => Ok(Version::HTTP_10),
        t if t.starts_with(b"HTTP/") => Err(ProtoError::VersionNotSupported),
        _ => Err(ProtoError::BadRequest("bad HTTP version")),
    }
}

/// Splits a head into its lines, rejecting stray CR/LF/NUL inside a line.
fn lines(head: &[u8]) -> Result<Vec<&[u8]>, ProtoError> {
    let body = head
        .strip_suffix(b"\r\n\r\n")
        .ok_or(ProtoError::BadRequest("head not terminated"))?;
    let parts: Vec<&[u8]> = body.split(|&b| b == b'\n').collect();
    let last = parts.len() - 1;
    let mut out = Vec::with_capacity(parts.len());
    for (i, part) in parts.into_iter().enumerate() {
        let line = if i == last {
            part
        } else {
            part.strip_suffix(b"\r")
                .ok_or(ProtoError::BadRequest("bare line feed"))?
        };
        if line.iter().any(|&b| b == b'\r' || b == 0) {
            return Err(ProtoError::BadRequest("stray control character"));
        }
        out.push(line);
    }
    Ok(out)
}

fn parse_headers(lines: &[&[u8]], limits: &Limits) -> Result<HeaderMap, ProtoError> {
    if lines.len() > limits.max_headers {
        return Err(ProtoError::HeadersTooLarge);
    }
    let mut map = HeaderMap::with_capacity(lines.len());
    for line in lines {
        if line.first().is_some_and(|&b| b == b' ' || b == b'\t') {
            return Err(ProtoError::BadRequest("obsolete line folding"));
        }
        let colon = line
            .iter()
            .position(|&b| b == b':')
            .ok_or(ProtoError::BadRequest("header without a colon"))?;
        let name = HeaderName::from_bytes(&line[..colon])
            .map_err(|_| ProtoError::BadRequest("bad header name"))?;
        let value = trim_ows(&line[colon + 1..]);
        let value = HeaderValue::from_bytes(value)
            .map_err(|_| ProtoError::BadRequest("bad header value"))?;
        map.append(name, value);
    }
    Ok(map)
}

const fn trim_ows(mut value: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = value {
        if *first == b' ' || *first == b'\t' {
            value = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = value {
        if *last == b' ' || *last == b'\t' {
            value = rest;
        } else {
            break;
        }
    }
    value
}

/// Parses a request head (`head` includes the closing blank line).
///
/// # Errors
///
/// A [`ProtoError`] for any syntax problem or limit breach.
pub fn parse_request_head(head: &[u8], limits: &Limits) -> Result<RequestHead, ProtoError> {
    if head.len() > limits.max_head_bytes {
        return Err(ProtoError::HeadersTooLarge);
    }
    let lines = lines(head)?;
    let (start, header_lines) = lines
        .split_first()
        .ok_or(ProtoError::BadRequest("empty request"))?;
    let mut parts = start.split(|&b| b == b' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(ProtoError::BadRequest("malformed request line"));
    };
    if target.len() > limits.max_uri_bytes {
        return Err(ProtoError::UriTooLong);
    }
    let method = Method::from_bytes(method).map_err(|_| ProtoError::BadRequest("bad method"))?;
    let target =
        std::str::from_utf8(target).map_err(|_| ProtoError::BadRequest("bad request target"))?;
    let uri = Uri::parse(target).map_err(|_| ProtoError::BadRequest("bad request target"))?;
    let version = parse_version(version)?;
    let headers = parse_headers(header_lines, limits)?;
    if version == Version::HTTP_11 {
        let mut hosts = headers.get_all("host");
        match (hosts.next(), hosts.next()) {
            (Some(_), None) => {}
            (None, _) => return Err(ProtoError::BadRequest("missing Host header")),
            (Some(_), Some(_)) => return Err(ProtoError::BadRequest("duplicate Host header")),
        }
    }
    Ok(RequestHead {
        method,
        uri,
        version,
        headers,
    })
}

/// Parses a response head. Reason phrases are discarded.
///
/// # Errors
///
/// A [`ProtoError`] for any syntax problem or limit breach.
pub fn parse_response_head(head: &[u8], limits: &Limits) -> Result<ResponseHead, ProtoError> {
    if head.len() > limits.max_head_bytes {
        return Err(ProtoError::HeadersTooLarge);
    }
    let lines = lines(head)?;
    let (start, header_lines) = lines
        .split_first()
        .ok_or(ProtoError::BadRequest("empty response"))?;
    let mut parts = start.splitn(3, |&b| b == b' ');
    let (Some(version), Some(status)) = (parts.next(), parts.next()) else {
        return Err(ProtoError::BadRequest("malformed status line"));
    };
    let version = parse_version(version)?;
    let status =
        StatusCode::from_bytes(status).map_err(|_| ProtoError::BadRequest("bad status code"))?;
    let headers = parse_headers(header_lines, limits)?;
    Ok(ResponseHead {
        status,
        version,
        headers,
    })
}

/// The one `Content-Length` a message declares, if any.
fn content_length(headers: &HeaderMap) -> Result<Option<u64>, ProtoError> {
    let mut found: Option<u64> = None;
    for value in headers.get_all("content-length") {
        let text = value
            .to_str()
            .map_err(|_| ProtoError::BadRequest("bad Content-Length"))?;
        for item in text.split(',') {
            let item = item.trim();
            if item.is_empty() || !item.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ProtoError::BadRequest("bad Content-Length"));
            }
            let n: u64 = item
                .parse()
                .map_err(|_| ProtoError::BadRequest("bad Content-Length"))?;
            if found.is_some_and(|prev| prev != n) {
                return Err(ProtoError::BadRequest("conflicting Content-Length"));
            }
            found = Some(n);
        }
    }
    Ok(found)
}

/// `Some(true)` when the final coding is `chunked`; an error for anything else.
fn transfer_chunked(headers: &HeaderMap) -> Result<bool, ProtoError> {
    let Some(joined) = headers.get_joined("transfer-encoding") else {
        return Ok(false);
    };
    let codings: Vec<String> = joined
        .split(',')
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    match codings.as_slice() {
        [only] if only == "chunked" => Ok(true),
        [.., last] if last == "chunked" => Err(ProtoError::NotImplemented),
        _ => Err(ProtoError::BadRequest(
            "Transfer-Encoding does not end in chunked",
        )),
    }
}

/// How the body of a request is delimited.
///
/// # Errors
///
/// A [`ProtoError`] when the headers are contradictory or unsupported.
pub fn request_framing(version: Version, headers: &HeaderMap) -> Result<Framing, ProtoError> {
    let length = content_length(headers)?;
    let chunked = headers.contains_key("transfer-encoding");
    if chunked {
        if version == Version::HTTP_10 {
            return Err(ProtoError::BadRequest("Transfer-Encoding on HTTP/1.0"));
        }
        if length.is_some() {
            return Err(ProtoError::BadRequest(
                "both Content-Length and Transfer-Encoding",
            ));
        }
        transfer_chunked(headers)?;
        return Ok(Framing::Chunked);
    }
    Ok(match length {
        Some(0) | None => Framing::None,
        Some(n) => Framing::Length(n),
    })
}

/// How the body of a response is delimited, given the request it answers.
///
/// # Errors
///
/// A [`ProtoError`] when the headers are contradictory or unsupported.
pub fn response_framing(
    request_method: &Method,
    status: StatusCode,
    headers: &HeaderMap,
) -> Result<Framing, ProtoError> {
    if *request_method == Method::HEAD
        || status.has_no_body()
        || (*request_method == Method::CONNECT && status.is_success())
    {
        return Ok(Framing::None);
    }
    if headers.contains_key("transfer-encoding") {
        // A response with both is framed by chunking; the caller drops the connection afterwards.
        return if transfer_chunked(headers)? {
            Ok(Framing::Chunked)
        } else {
            Err(ProtoError::BadRequest("bad Transfer-Encoding"))
        };
    }
    Ok(match content_length(headers)? {
        Some(0) => Framing::None,
        Some(n) => Framing::Length(n),
        None => Framing::UntilClose,
    })
}

/// Whether the peer wants the connection kept open after this message.
#[must_use]
pub fn wants_keep_alive(version: Version, headers: &HeaderMap) -> bool {
    let mut close = false;
    let mut keep = false;
    for value in headers.get_all("connection") {
        if let Ok(text) = value.to_str() {
            for token in text.split(',') {
                let token = token.trim();
                close |= token.eq_ignore_ascii_case("close");
                keep |= token.eq_ignore_ascii_case("keep-alive");
            }
        }
    }
    if close {
        return false;
    }
    version == Version::HTTP_11 || keep
}

/// Whether a header list (e.g. `Connection`) contains `token`.
#[must_use]
pub fn has_token(headers: &HeaderMap, name: &str, token: &str) -> bool {
    headers
        .get_all(name)
        .filter_map(|v| v.to_str().ok())
        .any(|text| {
            text.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case(token))
        })
}

/// Appends `HTTP/1.1 <code> <reason>\r\n` and the header lines and blank line.
pub fn write_response_head(out: &mut Vec<u8>, status: StatusCode, headers: &HeaderMap) {
    out.extend_from_slice(b"HTTP/1.1 ");
    out.extend_from_slice(status.as_u16().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(status.canonical_reason().unwrap_or("").as_bytes());
    out.extend_from_slice(b"\r\n");
    write_headers(out, headers);
}

/// Appends `<METHOD> <target> HTTP/1.1\r\n` and the header lines and blank line.
pub fn write_request_head(out: &mut Vec<u8>, method: &Method, target: &str, headers: &HeaderMap) {
    out.extend_from_slice(method.as_str().as_bytes());
    out.push(b' ');
    out.extend_from_slice(target.as_bytes());
    out.extend_from_slice(b" HTTP/1.1\r\n");
    write_headers(out, headers);
}

fn write_headers(out: &mut Vec<u8>, headers: &HeaderMap) {
    for (name, value) in headers {
        out.extend_from_slice(name.as_str().as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str) -> Result<RequestHead, ProtoError> {
        parse_request_head(text.as_bytes(), &Limits::default())
    }

    fn map(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[quench_test::test]
    fn parses_a_plain_get() {
        let head = req("GET /a/b?x=1 HTTP/1.1\r\nHost: example.com\r\nAccept:  text/html \r\n\r\n")
            .unwrap();
        assert_eq!(head.method, Method::GET);
        assert_eq!(head.uri.path(), "/a/b");
        assert_eq!(head.uri.query(), Some("x=1"));
        assert_eq!(head.version, Version::HTTP_11);
        assert_eq!(head.headers.get("accept").unwrap(), "text/html");
        assert_eq!(head.headers.get("HOST").unwrap(), "example.com");
    }

    #[quench_test::test]
    fn http_10_does_not_need_a_host() {
        assert!(req("GET / HTTP/1.0\r\n\r\n").is_ok());
    }

    #[quench_test::test]
    fn host_is_required_and_unique_on_http_11() {
        assert_eq!(
            req("GET / HTTP/1.1\r\n\r\n").unwrap_err(),
            ProtoError::BadRequest("missing Host header")
        );
        assert_eq!(
            req("GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n").unwrap_err(),
            ProtoError::BadRequest("duplicate Host header")
        );
    }

    #[quench_test::test]
    fn rejects_malformed_request_lines() {
        for bad in [
            "GET /\r\n\r\n",
            "GET  / HTTP/1.1\r\nHost: a\r\n\r\n",
            "GET / HTTP/1.1 extra\r\nHost: a\r\n\r\n",
            "G(T / HTTP/1.1\r\nHost: a\r\n\r\n",
            "GET /a b HTTP/1.1\r\nHost: a\r\n\r\n",
            "GET / HTTX/1.1\r\nHost: a\r\n\r\n",
            "\r\n\r\n",
        ] {
            assert!(
                matches!(req(bad), Err(ProtoError::BadRequest(_))),
                "{bad:?}"
            );
        }
        assert_eq!(
            req("GET / HTTP/2.0\r\nHost: a\r\n\r\n").unwrap_err(),
            ProtoError::VersionNotSupported
        );
        assert_eq!(
            req("GET / HTTP/1.2\r\nHost: a\r\n\r\n").unwrap_err(),
            ProtoError::VersionNotSupported
        );
    }

    #[quench_test::test]
    fn rejects_smuggling_vectors_in_header_syntax() {
        for bad in [
            "GET / HTTP/1.1\r\nHost: a\r\n X: folded\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\r\nX : space-before-colon\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\r\nno colon here\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\nX: bare-lf\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\r\nX: a\rb\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\r\nX: a\0b\r\n\r\n",
            "GET / HTTP/1.1\r\nHost: a\r\n: empty-name\r\n\r\n",
        ] {
            assert!(
                matches!(req(bad), Err(ProtoError::BadRequest(_))),
                "{bad:?}"
            );
        }
    }

    #[quench_test::test]
    fn enforces_the_limits() {
        let tiny = Limits {
            max_head_bytes: 40,
            max_headers: 2,
            max_uri_bytes: 8,
        };
        assert_eq!(
            parse_request_head(b"GET /123456789 HTTP/1.1\r\nHost: a\r\n\r\n", &tiny).unwrap_err(),
            ProtoError::UriTooLong
        );
        assert_eq!(
            parse_request_head(
                b"GET / HTTP/1.1\r\nHost: a\r\nA: b\r\nB: c\r\n\r\n",
                &Limits {
                    max_headers: 2,
                    ..Limits::default()
                }
            )
            .unwrap_err(),
            ProtoError::HeadersTooLarge
        );
        assert_eq!(
            parse_request_head(
                b"GET / HTTP/1.1\r\nHost: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n\r\n",
                &tiny
            )
            .unwrap_err(),
            ProtoError::HeadersTooLarge
        );
        assert_eq!(ProtoError::UriTooLong.status(), StatusCode::URI_TOO_LONG);
        assert_eq!(
            ProtoError::HeadersTooLarge.status(),
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
        );
        assert_eq!(
            ProtoError::VersionNotSupported.status(),
            StatusCode::HTTP_VERSION_NOT_SUPPORTED
        );
        assert_eq!(
            ProtoError::NotImplemented.status(),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ProtoError::BadRequest("x").status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[quench_test::test]
    fn find_head_end_needs_the_blank_line() {
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\nHost: a\r\n"), None);
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nrest"), Some(18));
    }

    #[quench_test::test]
    fn head_must_be_terminated() {
        assert!(matches!(
            lines(b"GET / HTTP/1.1\r\n"),
            Err(ProtoError::BadRequest(_))
        ));
    }

    #[quench_test::test]
    fn parses_response_heads() {
        let head = parse_response_head(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\n\r\n",
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(head.status, StatusCode::NOT_FOUND);
        assert_eq!(head.headers.get("content-length").unwrap(), "3");
        let no_reason = parse_response_head(b"HTTP/1.0 200 \r\n\r\n", &Limits::default()).unwrap();
        assert_eq!(no_reason.status, StatusCode::OK);
        let bare = parse_response_head(b"HTTP/1.1 204\r\n\r\n", &Limits::default()).unwrap();
        assert_eq!(bare.status, StatusCode::NO_CONTENT);
        for bad in [
            &b"HTTP/1.1\r\n\r\n"[..],
            b"HTTP/1.1 20 OK\r\n\r\n",
            b"HTTP/1.1 abc OK\r\n\r\n",
            b"nope 200 OK\r\n\r\n",
            b"\r\n\r\n",
        ] {
            assert!(
                parse_response_head(bad, &Limits::default()).is_err(),
                "{bad:?}"
            );
        }
        assert!(
            parse_response_head(
                b"HTTP/1.1 200 OK\r\nA: b\r\n\r\n",
                &Limits {
                    max_head_bytes: 5,
                    ..Limits::default()
                }
            )
            .is_err()
        );
    }

    #[quench_test::test]
    fn request_framing_follows_the_headers() {
        let v = Version::HTTP_11;
        assert_eq!(request_framing(v, &map(&[])).unwrap(), Framing::None);
        assert_eq!(
            request_framing(v, &map(&[("content-length", "0")])).unwrap(),
            Framing::None
        );
        assert_eq!(
            request_framing(v, &map(&[("content-length", "12")])).unwrap(),
            Framing::Length(12)
        );
        assert_eq!(
            request_framing(v, &map(&[("content-length", "7"), ("content-length", "7")])).unwrap(),
            Framing::Length(7)
        );
        assert_eq!(
            request_framing(v, &map(&[("content-length", "7, 7")])).unwrap(),
            Framing::Length(7)
        );
        assert_eq!(
            request_framing(v, &map(&[("transfer-encoding", "Chunked")])).unwrap(),
            Framing::Chunked
        );
    }

    #[quench_test::test]
    fn request_framing_rejects_smuggling_combinations() {
        let v = Version::HTTP_11;
        for headers in [
            map(&[("content-length", "5"), ("transfer-encoding", "chunked")]),
            map(&[("content-length", "5"), ("content-length", "6")]),
            map(&[("content-length", "5, 6")]),
            map(&[("content-length", "-5")]),
            map(&[("content-length", "+5")]),
            map(&[("content-length", "5 5")]),
            map(&[("content-length", "")]),
            map(&[("content-length", "99999999999999999999999")]),
            map(&[("transfer-encoding", "gzip")]),
            map(&[("transfer-encoding", "chunked, gzip")]),
            map(&[("transfer-encoding", "chunked, chunked")]),
        ] {
            assert!(request_framing(v, &headers).is_err(), "{headers:?}");
        }
        assert_eq!(
            request_framing(v, &map(&[("transfer-encoding", "gzip, chunked")])).unwrap_err(),
            ProtoError::NotImplemented
        );
        assert!(
            request_framing(Version::HTTP_10, &map(&[("transfer-encoding", "chunked")])).is_err()
        );
    }

    #[quench_test::test]
    fn response_framing_follows_method_status_and_headers() {
        let get = Method::GET;
        let ok = StatusCode::OK;
        assert_eq!(
            response_framing(&Method::HEAD, ok, &map(&[("content-length", "9")])).unwrap(),
            Framing::None
        );
        assert_eq!(
            response_framing(&get, StatusCode::NO_CONTENT, &map(&[])).unwrap(),
            Framing::None
        );
        assert_eq!(
            response_framing(&get, StatusCode::NOT_MODIFIED, &map(&[])).unwrap(),
            Framing::None
        );
        assert_eq!(
            response_framing(&Method::CONNECT, ok, &map(&[])).unwrap(),
            Framing::None
        );
        assert_eq!(
            response_framing(&get, ok, &map(&[("content-length", "0")])).unwrap(),
            Framing::None
        );
        assert_eq!(
            response_framing(&get, ok, &map(&[("content-length", "4")])).unwrap(),
            Framing::Length(4)
        );
        assert_eq!(
            response_framing(&get, ok, &map(&[("transfer-encoding", "chunked")])).unwrap(),
            Framing::Chunked
        );
        assert_eq!(
            response_framing(&get, ok, &map(&[])).unwrap(),
            Framing::UntilClose
        );
        assert!(response_framing(&get, ok, &map(&[("transfer-encoding", "gzip")])).is_err());
        assert!(response_framing(&get, ok, &map(&[("content-length", "x")])).is_err());
    }

    #[quench_test::test]
    fn keep_alive_rules() {
        assert!(wants_keep_alive(Version::HTTP_11, &map(&[])));
        assert!(!wants_keep_alive(
            Version::HTTP_11,
            &map(&[("connection", "close")])
        ));
        assert!(!wants_keep_alive(
            Version::HTTP_11,
            &map(&[("connection", "keep-alive, Close")])
        ));
        assert!(!wants_keep_alive(Version::HTTP_10, &map(&[])));
        assert!(wants_keep_alive(
            Version::HTTP_10,
            &map(&[("connection", "Keep-Alive")])
        ));
        assert!(has_token(
            &map(&[("connection", "keep-alive, Upgrade")]),
            "connection",
            "upgrade"
        ));
        assert!(!has_token(&map(&[]), "connection", "upgrade"));
    }

    #[quench_test::test]
    fn heads_serialise_round_trip() {
        let headers = map(&[
            ("content-type", "text/plain"),
            ("set-cookie", "a=1"),
            ("set-cookie", "b=2"),
        ]);
        let mut out = Vec::new();
        write_response_head(&mut out, StatusCode::CREATED, &headers);
        assert_eq!(
            String::from_utf8(out.clone()).unwrap(),
            "HTTP/1.1 201 Created\r\ncontent-type: text/plain\r\nset-cookie: a=1\r\nset-cookie: b=2\r\n\r\n"
        );
        let parsed = parse_response_head(&out, &Limits::default()).unwrap();
        assert_eq!(parsed.status, StatusCode::CREATED);
        assert_eq!(parsed.headers, headers);

        let mut out = Vec::new();
        write_request_head(&mut out, &Method::POST, "/x?y=1", &map(&[("host", "h")]));
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "POST /x?y=1 HTTP/1.1\r\nhost: h\r\n\r\n"
        );
        let mut unknown = Vec::new();
        write_response_head(
            &mut unknown,
            StatusCode::from_u16(599).unwrap(),
            &HeaderMap::new(),
        );
        assert!(
            String::from_utf8(unknown)
                .unwrap()
                .starts_with("HTTP/1.1 599 \r\n")
        );
    }

    #[quench_test::test]
    fn proto_errors_display() {
        assert!(ProtoError::BadRequest("why").to_string().contains("why"));
        assert!(ProtoError::UriTooLong.to_string().contains("too long"));
        assert!(
            ProtoError::HeadersTooLarge
                .to_string()
                .contains("too large")
        );
        assert!(
            ProtoError::VersionNotSupported
                .to_string()
                .contains("version")
        );
        assert!(ProtoError::NotImplemented.to_string().contains("coding"));
    }
}
