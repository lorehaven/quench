//! Header names, values and the ordered header map.

use quench_core::bytes::Bytes;
use std::borrow::Cow;
use std::fmt;

/// Whether `b` may appear in a token (RFC 9110 section 5.6.2).
#[must_use]
pub const fn is_token_byte(b: u8) -> bool {
    matches!(b,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z')
}

/// A text was not a valid header name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidHeaderName;

impl fmt::Display for InvalidHeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP header name")
    }
}

impl std::error::Error for InvalidHeaderName {}

/// A text was not a valid header value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidHeaderValue;

impl fmt::Display for InvalidHeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to parse header value")
    }
}

impl std::error::Error for InvalidHeaderValue {}

/// A header field name: a token, stored lowercase.
#[derive(Clone)]
pub struct HeaderName(Repr);

#[derive(Clone)]
enum Repr {
    Static(&'static str),
    Owned(Box<str>),
}

impl HeaderName {
    const fn from_static_unchecked(name: &'static str) -> Self {
        Self(Repr::Static(name))
    }

    /// A name from a `'static` string.
    ///
    /// # Panics
    ///
    /// Panics when `name` is empty, has a non-token byte, or contains an uppercase letter.
    #[must_use]
    pub fn from_static(name: &'static str) -> Self {
        assert!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| is_token_byte(b) && !b.is_ascii_uppercase()),
            "invalid header name {name:?}: must be a lowercase token"
        );
        Self(Repr::Static(name))
    }

    /// Parses a name, folding it to lowercase.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidHeaderName`] for an empty slice or a non-token byte.
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderName> {
        if src.is_empty() || !src.iter().all(|&b| is_token_byte(b)) {
            return Err(InvalidHeaderName);
        }
        let lower: String = src
            .iter()
            .map(|b| char::from(b.to_ascii_lowercase()))
            .collect();
        Ok(Self(Repr::Owned(lower.into_boxed_str())))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Static(name) => name,
            Repr::Owned(name) => name,
        }
    }
}

impl PartialEq for HeaderName {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for HeaderName {}

impl std::hash::Hash for HeaderName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialOrd for HeaderName {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeaderName {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl AsRef<str> for HeaderName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::str::FromStr for HeaderName {
    type Err = InvalidHeaderName;

    fn from_str(s: &str) -> Result<Self, InvalidHeaderName> {
        Self::from_bytes(s.as_bytes())
    }
}

impl TryFrom<&str> for HeaderName {
    type Error = InvalidHeaderName;

    fn try_from(s: &str) -> Result<Self, InvalidHeaderName> {
        Self::from_bytes(s.as_bytes())
    }
}

impl PartialEq<str> for HeaderName {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}

impl PartialEq<&str> for HeaderName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}

/// A header field value: visible ASCII, spaces, tabs and obs-text, never a control byte.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct HeaderValue(Bytes);

const fn is_value_byte(b: u8) -> bool {
    b == b'\t' || (b >= 0x20 && b != 0x7f)
}

impl HeaderValue {
    /// A value from a `'static` string.
    ///
    /// # Panics
    ///
    /// Panics when `src` contains a byte that is not allowed in a value.
    #[must_use]
    pub fn from_static(src: &'static str) -> Self {
        assert!(
            src.bytes().all(is_value_byte),
            "invalid header value {src:?}"
        );
        Self(Bytes::from_static(src.as_bytes()))
    }

    /// # Errors
    ///
    /// Returns [`InvalidHeaderValue`] when `src` has a control byte (including CR and LF).
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidHeaderValue> {
        if src.iter().all(|&b| is_value_byte(b)) {
            Ok(Self(Bytes::copy_from_slice(src)))
        } else {
            Err(InvalidHeaderValue)
        }
    }

    /// # Errors
    ///
    /// Returns [`InvalidHeaderValue`] when `src` has a control byte (including CR and LF).
    #[allow(clippy::should_implement_trait)] // mirrors `http::HeaderValue::from_str`, which is fallible
    pub fn from_str(src: &str) -> Result<Self, InvalidHeaderValue> {
        Self::from_bytes(src.as_bytes())
    }

    /// The value as text; fails when it holds obs-text bytes that are not UTF-8.
    ///
    /// # Errors
    ///
    /// Returns [`ToStrError`] for a value with non-ASCII bytes.
    pub fn to_str(&self) -> Result<&str, ToStrError> {
        if self.0.is_ascii() {
            // ASCII is always valid UTF-8.
            std::str::from_utf8(&self.0).map_err(|_| ToStrError)
        } else {
            Err(ToStrError)
        }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The value held bytes outside visible ASCII.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToStrError;

impl fmt::Display for ToStrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("header value is not visible ASCII")
    }
}

impl std::error::Error for ToStrError {}

impl fmt::Debug for HeaderValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\"", String::from_utf8_lossy(&self.0).escape_debug())
    }
}

impl std::str::FromStr for HeaderValue {
    type Err = InvalidHeaderValue;

    fn from_str(s: &str) -> Result<Self, InvalidHeaderValue> {
        Self::from_bytes(s.as_bytes())
    }
}

impl TryFrom<&str> for HeaderValue {
    type Error = InvalidHeaderValue;

    fn try_from(s: &str) -> Result<Self, InvalidHeaderValue> {
        Self::from_bytes(s.as_bytes())
    }
}

impl TryFrom<String> for HeaderValue {
    type Error = InvalidHeaderValue;

    fn try_from(s: String) -> Result<Self, InvalidHeaderValue> {
        Self::from_bytes(s.as_bytes())
    }
}

macro_rules! from_number {
    ($($ty:ty),*) => {$(
        impl From<$ty> for HeaderValue {
            fn from(n: $ty) -> Self {
                Self(Bytes::from(n.to_string()))
            }
        }
    )*};
}
from_number!(u16, u32, u64, usize, i32, i64);

impl AsRef<[u8]> for HeaderValue {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl PartialEq<str> for HeaderValue {
    fn eq(&self, other: &str) -> bool {
        self.0.as_ref() == other.as_bytes()
    }
}

impl PartialEq<&str> for HeaderValue {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == other.as_bytes()
    }
}

/// Anything that can name a header in a lookup: a `&str` (any case), a [`HeaderName`] or a reference to one.
pub trait AsHeaderName {
    #[doc(hidden)]
    fn matches(&self, name: &HeaderName) -> bool;
}

impl AsHeaderName for str {
    fn matches(&self, name: &HeaderName) -> bool {
        name.as_str().eq_ignore_ascii_case(self)
    }
}

impl AsHeaderName for &str {
    fn matches(&self, name: &HeaderName) -> bool {
        name.as_str().eq_ignore_ascii_case(self)
    }
}

impl AsHeaderName for String {
    fn matches(&self, name: &HeaderName) -> bool {
        name.as_str().eq_ignore_ascii_case(self)
    }
}

impl AsHeaderName for HeaderName {
    fn matches(&self, name: &HeaderName) -> bool {
        self == name
    }
}

impl AsHeaderName for &HeaderName {
    fn matches(&self, name: &HeaderName) -> bool {
        *self == name
    }
}

/// Anything that can become a [`HeaderName`] when inserting: a name, or a `&str` that is parsed.
pub trait IntoHeaderName {
    #[doc(hidden)]
    fn into_header_name(self) -> Option<HeaderName>;
}

impl IntoHeaderName for HeaderName {
    fn into_header_name(self) -> Option<HeaderName> {
        Some(self)
    }
}

impl IntoHeaderName for &HeaderName {
    fn into_header_name(self) -> Option<HeaderName> {
        Some(self.clone())
    }
}

impl IntoHeaderName for &str {
    fn into_header_name(self) -> Option<HeaderName> {
        HeaderName::from_bytes(self.as_bytes()).ok()
    }
}

/// An ordered multimap of headers. Lookups are linear: requests are capped at a
/// hundred or so headers, where a scan beats hashing.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct HeaderMap {
    entries: Vec<(HeaderName, HeaderValue)>,
}

impl HeaderMap {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Number of header lines (a name with two values counts twice).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The first value for `name`.
    #[must_use]
    pub fn get<K: AsHeaderName + ?Sized>(&self, name: &K) -> Option<&HeaderValue> {
        self.entries
            .iter()
            .find(|(n, _)| name.matches(n))
            .map(|(_, v)| v)
    }

    /// Every value for `name`, in insertion order.
    pub fn get_all<'a, K: AsHeaderName + ?Sized>(
        &'a self,
        name: &'a K,
    ) -> impl Iterator<Item = &'a HeaderValue> {
        self.entries
            .iter()
            .filter(move |(n, _)| name.matches(n))
            .map(|(_, v)| v)
    }

    #[must_use]
    pub fn contains_key<K: AsHeaderName + ?Sized>(&self, name: &K) -> bool {
        self.entries.iter().any(|(n, _)| name.matches(n))
    }

    /// Replaces every value of `name` with `value`, returning the first old one.
    /// A `&str` name that is not a valid token is ignored.
    pub fn insert<K: IntoHeaderName>(
        &mut self,
        name: K,
        value: HeaderValue,
    ) -> Option<HeaderValue> {
        let name = name.into_header_name()?;
        let first = self.entries.iter().position(|(n, _)| *n == name);
        let Some(first) = first else {
            self.entries.push((name, value));
            return None;
        };
        let old = std::mem::replace(&mut self.entries[first].1, value);
        let mut index = 0;
        self.entries.retain(|(n, _)| {
            let keep = index <= first || *n != name;
            index += 1;
            keep
        });
        Some(old)
    }

    /// Adds a value after any existing ones. Returns whether `name` was already present.
    pub fn append<K: IntoHeaderName>(&mut self, name: K, value: HeaderValue) -> bool {
        let Some(name) = name.into_header_name() else {
            return false;
        };
        let present = self.entries.iter().any(|(n, _)| *n == name);
        self.entries.push((name, value));
        present
    }

    /// Removes every value of `name`, returning the first.
    pub fn remove<K: AsHeaderName + ?Sized>(&mut self, name: &K) -> Option<HeaderValue> {
        let first = self.entries.iter().position(|(n, _)| name.matches(n))?;
        let (_, value) = self.entries.remove(first);
        self.entries.retain(|(n, _)| !name.matches(n));
        Some(value)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        self.entries.iter().map(|(n, v)| (n, v))
    }

    /// Distinct names in first-seen order.
    pub fn keys(&self) -> impl Iterator<Item = &HeaderName> {
        self.entries.iter().enumerate().filter_map(|(i, (n, _))| {
            (!self.entries[..i].iter().any(|(earlier, _)| earlier == n)).then_some(n)
        })
    }

    pub fn values(&self) -> impl Iterator<Item = &HeaderValue> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// The values of `name` joined with `, ` - how a list-valued header is read when it was sent as several lines.
    #[must_use]
    pub fn get_joined<K: AsHeaderName + ?Sized>(&self, name: &K) -> Option<Cow<'_, str>> {
        let mut values = self
            .entries
            .iter()
            .filter(|(n, _)| name.matches(n))
            .filter_map(|(_, v)| v.to_str().ok());
        let first = values.next()?;
        let Some(second) = values.next() else {
            return Some(Cow::Borrowed(first));
        };
        let mut joined = format!("{first}, {second}");
        for rest in values {
            joined.push_str(", ");
            joined.push_str(rest);
        }
        Some(Cow::Owned(joined))
    }
}

impl fmt::Debug for HeaderMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for &'a HeaderMap {
    type Item = (&'a HeaderName, &'a HeaderValue);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (HeaderName, HeaderValue)>,
        fn(&'a (HeaderName, HeaderValue)) -> (&'a HeaderName, &'a HeaderValue),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(n, v)| (n, v))
    }
}

impl IntoIterator for HeaderMap {
    type Item = (HeaderName, HeaderValue);
    type IntoIter = std::vec::IntoIter<(HeaderName, HeaderValue)>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl FromIterator<(HeaderName, HeaderValue)> for HeaderMap {
    fn from_iter<I: IntoIterator<Item = (HeaderName, HeaderValue)>>(iter: I) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
    }
}

impl Extend<(HeaderName, HeaderValue)> for HeaderMap {
    fn extend<I: IntoIterator<Item = (HeaderName, HeaderValue)>>(&mut self, iter: I) {
        self.entries.extend(iter);
    }
}

/// Well-known header names.
pub const ACCEPT: HeaderName = HeaderName::from_static_unchecked("accept");
pub const ACCEPT_CHARSET: HeaderName = HeaderName::from_static_unchecked("accept-charset");
pub const ACCEPT_ENCODING: HeaderName = HeaderName::from_static_unchecked("accept-encoding");
pub const ACCEPT_LANGUAGE: HeaderName = HeaderName::from_static_unchecked("accept-language");
pub const ACCEPT_RANGES: HeaderName = HeaderName::from_static_unchecked("accept-ranges");
pub const ACCESS_CONTROL_ALLOW_CREDENTIALS: HeaderName =
    HeaderName::from_static_unchecked("access-control-allow-credentials");
pub const ACCESS_CONTROL_ALLOW_HEADERS: HeaderName =
    HeaderName::from_static_unchecked("access-control-allow-headers");
pub const ACCESS_CONTROL_ALLOW_METHODS: HeaderName =
    HeaderName::from_static_unchecked("access-control-allow-methods");
pub const ACCESS_CONTROL_ALLOW_ORIGIN: HeaderName =
    HeaderName::from_static_unchecked("access-control-allow-origin");
pub const ACCESS_CONTROL_EXPOSE_HEADERS: HeaderName =
    HeaderName::from_static_unchecked("access-control-expose-headers");
pub const ACCESS_CONTROL_MAX_AGE: HeaderName =
    HeaderName::from_static_unchecked("access-control-max-age");
pub const ACCESS_CONTROL_REQUEST_HEADERS: HeaderName =
    HeaderName::from_static_unchecked("access-control-request-headers");
pub const ACCESS_CONTROL_REQUEST_METHOD: HeaderName =
    HeaderName::from_static_unchecked("access-control-request-method");
pub const AGE: HeaderName = HeaderName::from_static_unchecked("age");
pub const ALLOW: HeaderName = HeaderName::from_static_unchecked("allow");
pub const AUTHORIZATION: HeaderName = HeaderName::from_static_unchecked("authorization");
pub const CACHE_CONTROL: HeaderName = HeaderName::from_static_unchecked("cache-control");
pub const CONNECTION: HeaderName = HeaderName::from_static_unchecked("connection");
pub const CONTENT_DISPOSITION: HeaderName =
    HeaderName::from_static_unchecked("content-disposition");
pub const CONTENT_ENCODING: HeaderName = HeaderName::from_static_unchecked("content-encoding");
pub const CONTENT_LANGUAGE: HeaderName = HeaderName::from_static_unchecked("content-language");
pub const CONTENT_LENGTH: HeaderName = HeaderName::from_static_unchecked("content-length");
pub const CONTENT_LOCATION: HeaderName = HeaderName::from_static_unchecked("content-location");
pub const CONTENT_RANGE: HeaderName = HeaderName::from_static_unchecked("content-range");
pub const CONTENT_SECURITY_POLICY: HeaderName =
    HeaderName::from_static_unchecked("content-security-policy");
pub const CONTENT_TYPE: HeaderName = HeaderName::from_static_unchecked("content-type");
pub const COOKIE: HeaderName = HeaderName::from_static_unchecked("cookie");
pub const DATE: HeaderName = HeaderName::from_static_unchecked("date");
pub const ETAG: HeaderName = HeaderName::from_static_unchecked("etag");
pub const EXPECT: HeaderName = HeaderName::from_static_unchecked("expect");
pub const EXPIRES: HeaderName = HeaderName::from_static_unchecked("expires");
pub const FORWARDED: HeaderName = HeaderName::from_static_unchecked("forwarded");
pub const FROM: HeaderName = HeaderName::from_static_unchecked("from");
pub const HOST: HeaderName = HeaderName::from_static_unchecked("host");
pub const IF_MATCH: HeaderName = HeaderName::from_static_unchecked("if-match");
pub const IF_MODIFIED_SINCE: HeaderName = HeaderName::from_static_unchecked("if-modified-since");
pub const IF_NONE_MATCH: HeaderName = HeaderName::from_static_unchecked("if-none-match");
pub const IF_RANGE: HeaderName = HeaderName::from_static_unchecked("if-range");
pub const IF_UNMODIFIED_SINCE: HeaderName =
    HeaderName::from_static_unchecked("if-unmodified-since");
pub const LAST_MODIFIED: HeaderName = HeaderName::from_static_unchecked("last-modified");
pub const LINK: HeaderName = HeaderName::from_static_unchecked("link");
pub const LOCATION: HeaderName = HeaderName::from_static_unchecked("location");
pub const ORIGIN: HeaderName = HeaderName::from_static_unchecked("origin");
pub const PRAGMA: HeaderName = HeaderName::from_static_unchecked("pragma");
pub const PROXY_AUTHENTICATE: HeaderName = HeaderName::from_static_unchecked("proxy-authenticate");
pub const PROXY_AUTHORIZATION: HeaderName =
    HeaderName::from_static_unchecked("proxy-authorization");
pub const RANGE: HeaderName = HeaderName::from_static_unchecked("range");
pub const REFERER: HeaderName = HeaderName::from_static_unchecked("referer");
pub const REFERRER_POLICY: HeaderName = HeaderName::from_static_unchecked("referrer-policy");
pub const RETRY_AFTER: HeaderName = HeaderName::from_static_unchecked("retry-after");
pub const SEC_WEBSOCKET_ACCEPT: HeaderName =
    HeaderName::from_static_unchecked("sec-websocket-accept");
pub const SEC_WEBSOCKET_KEY: HeaderName = HeaderName::from_static_unchecked("sec-websocket-key");
pub const SEC_WEBSOCKET_PROTOCOL: HeaderName =
    HeaderName::from_static_unchecked("sec-websocket-protocol");
pub const SEC_WEBSOCKET_VERSION: HeaderName =
    HeaderName::from_static_unchecked("sec-websocket-version");
pub const SERVER: HeaderName = HeaderName::from_static_unchecked("server");
pub const SET_COOKIE: HeaderName = HeaderName::from_static_unchecked("set-cookie");
pub const STRICT_TRANSPORT_SECURITY: HeaderName =
    HeaderName::from_static_unchecked("strict-transport-security");
pub const TE: HeaderName = HeaderName::from_static_unchecked("te");
pub const TRAILER: HeaderName = HeaderName::from_static_unchecked("trailer");
pub const TRANSFER_ENCODING: HeaderName = HeaderName::from_static_unchecked("transfer-encoding");
pub const UPGRADE: HeaderName = HeaderName::from_static_unchecked("upgrade");
pub const USER_AGENT: HeaderName = HeaderName::from_static_unchecked("user-agent");
pub const VARY: HeaderName = HeaderName::from_static_unchecked("vary");
pub const VIA: HeaderName = HeaderName::from_static_unchecked("via");
pub const WWW_AUTHENTICATE: HeaderName = HeaderName::from_static_unchecked("www-authenticate");
pub const X_CONTENT_TYPE_OPTIONS: HeaderName =
    HeaderName::from_static_unchecked("x-content-type-options");
pub const X_FORWARDED_FOR: HeaderName = HeaderName::from_static_unchecked("x-forwarded-for");
pub const X_FORWARDED_HOST: HeaderName = HeaderName::from_static_unchecked("x-forwarded-host");
pub const X_FORWARDED_PROTO: HeaderName = HeaderName::from_static_unchecked("x-forwarded-proto");
pub const X_FRAME_OPTIONS: HeaderName = HeaderName::from_static_unchecked("x-frame-options");
pub const X_REQUEST_ID: HeaderName = HeaderName::from_static_unchecked("x-request-id");

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str) -> HeaderValue {
        HeaderValue::from_str(text).unwrap()
    }

    #[test]
    fn names_are_lowercased_tokens() {
        assert_eq!(
            HeaderName::from_bytes(b"Content-Type").unwrap(),
            CONTENT_TYPE
        );
        assert_eq!(CONTENT_TYPE.as_str(), "content-type");
        assert_eq!(CONTENT_TYPE.to_string(), "content-type");
        assert_eq!(format!("{CONTENT_TYPE:?}"), "\"content-type\"");
        assert!(CONTENT_TYPE == "Content-Type" && CONTENT_TYPE == *"content-TYPE");
        assert_eq!(
            "X-Custom".parse::<HeaderName>().unwrap().as_str(),
            "x-custom"
        );
        assert_eq!(HeaderName::try_from("a-b").unwrap().as_ref() as &str, "a-b");
        assert_eq!(HeaderName::from_static("x-static").as_str(), "x-static");
        for bad in [&b""[..], b"a b", b"a:b", b"a\r\n", b"caf\xc3\xa9"] {
            assert_eq!(
                HeaderName::from_bytes(bad),
                Err(InvalidHeaderName),
                "{bad:?}"
            );
        }
        assert!(InvalidHeaderName.to_string().contains("invalid"));
    }

    #[test]
    #[should_panic(expected = "invalid header name")]
    fn from_static_rejects_uppercase() {
        let _ = HeaderName::from_static("Upper");
    }

    #[test]
    fn values_reject_control_bytes() {
        assert_eq!(
            value("text/html; charset=utf-8").to_str().unwrap(),
            "text/html; charset=utf-8"
        );
        assert!(HeaderValue::from_str("tab\there").is_ok());
        for bad in ["a\nb", "a\rb", "a\0b", "a\x7fb"] {
            assert_eq!(
                HeaderValue::from_str(bad),
                Err(InvalidHeaderValue),
                "{bad:?}"
            );
        }
        assert!(InvalidHeaderValue.to_string().contains("header value"));
        assert!("ok".parse::<HeaderValue>().is_ok());
        assert!(HeaderValue::try_from("ok".to_string()).is_ok());
        assert!(HeaderValue::try_from("bad\n").is_err());
        assert!(HeaderValue::try_from("fine").is_ok());
    }

    #[test]
    fn values_expose_bytes_and_text() {
        let v = value("abc");
        assert_eq!(v.as_bytes(), b"abc");
        assert_eq!(v.len(), 3);
        assert!(!v.is_empty() && HeaderValue::from_static("").is_empty());
        assert!(v == "abc" && v == *"abc");
        assert_eq!(v.as_ref() as &[u8], b"abc");
        assert_eq!(format!("{v:?}"), "\"abc\"");
        let obs = HeaderValue::from_bytes(b"caf\xe9").unwrap();
        assert!(obs.to_str().is_err());
        assert!(ToStrError.to_string().contains("ASCII"));
        assert_eq!(HeaderValue::from(42u16).as_bytes(), b"42");
        assert_eq!(HeaderValue::from(42usize).as_bytes(), b"42");
        assert_eq!(HeaderValue::from(-4i32).as_bytes(), b"-4");
    }

    #[test]
    #[should_panic(expected = "invalid header value")]
    fn from_static_value_rejects_newlines() {
        let _ = HeaderValue::from_static("a\nb");
    }

    #[test]
    fn map_lookup_is_case_insensitive_and_ordered() {
        let mut map = HeaderMap::new();
        assert!(map.is_empty());
        assert!(!map.append("Set-Cookie", value("a=1")));
        assert!(map.append("set-cookie", value("b=2")));
        map.insert("Content-Type", value("text/plain"));
        assert_eq!(map.len(), 3);
        assert_eq!(map.get("SET-COOKIE").unwrap(), "a=1");
        assert_eq!(map.get(&CONTENT_TYPE).unwrap(), "text/plain");
        assert_eq!(
            map.get(&String::from("content-type")).unwrap(),
            "text/plain"
        );
        assert_eq!(map.get_all("set-cookie").count(), 2);
        assert!(map.contains_key("Content-Type") && !map.contains_key("x-none"));
        assert_eq!(
            map.keys().map(HeaderName::as_str).collect::<Vec<_>>(),
            ["set-cookie", "content-type"]
        );
        assert_eq!(map.values().count(), 3);
        assert_eq!(map.iter().next().unwrap().0.as_str(), "set-cookie");
        assert!(format!("{map:?}").contains("content-type"));
    }

    #[test]
    fn insert_replaces_every_earlier_value() {
        let mut map = HeaderMap::new();
        map.append("x", value("1"));
        map.append("y", value("mid"));
        map.append("x", value("2"));
        let old = map.insert("X", value("3"));
        assert_eq!(old.unwrap(), "1");
        assert_eq!(map.get_all("x").count(), 1);
        assert_eq!(map.get("x").unwrap(), "3");
        assert_eq!(map.len(), 2);
        assert!(map.insert("fresh", value("v")).is_none());
        assert!(map.insert("bad name", value("v")).is_none());
        assert!(!map.append("bad name", value("v")));
        assert!(!map.contains_key("bad name"));
        assert!(map.insert(&CONTENT_TYPE, value("a")).is_none());
        assert!(map.insert(&CONTENT_TYPE, value("b")).is_some());
    }

    #[test]
    fn remove_clears_all_values() {
        let mut map = HeaderMap::new();
        map.append("x", value("1"));
        map.append("x", value("2"));
        map.append("y", value("3"));
        assert_eq!(map.remove("x").unwrap(), "1");
        assert!(!map.contains_key("x") && map.len() == 1);
        assert!(map.remove("x").is_none());
        map.clear();
        assert!(map.is_empty());
    }

    #[test]
    fn joined_values_of_list_headers() {
        let mut map = HeaderMap::with_capacity(4);
        assert!(map.get_joined("accept").is_none());
        map.append("accept", value("a"));
        assert!(matches!(map.get_joined("accept"), Some(Cow::Borrowed("a"))));
        map.append("accept", value("b"));
        map.append("accept", value("c"));
        assert_eq!(map.get_joined("accept").unwrap(), "a, b, c");
        let mut obs = HeaderMap::new();
        obs.append("x", HeaderValue::from_bytes(b"\xe9").unwrap());
        assert!(obs.get_joined("x").is_none());
    }

    #[test]
    fn collects_extends_and_iterates() {
        let mut map: HeaderMap = [(CONTENT_TYPE, value("a")), (HOST, value("h"))]
            .into_iter()
            .collect();
        map.extend([(ACCEPT, value("*/*"))]);
        assert_eq!((&map).into_iter().count(), 3);
        assert_eq!(map.clone().into_iter().count(), 3);
        assert_eq!(map, map.clone());
        assert_eq!(map.get_all(&HOST).count(), 1);
    }

    #[test]
    fn well_known_constants_are_valid_lowercase_names() {
        for name in [
            &ACCEPT,
            &AUTHORIZATION,
            &CONTENT_LENGTH,
            &COOKIE,
            &LOCATION,
            &SET_COOKIE,
            &TRANSFER_ENCODING,
            &X_FORWARDED_FOR,
            &SEC_WEBSOCKET_KEY,
        ] {
            assert!(
                !name.as_str().is_empty()
                    && name
                        .as_str()
                        .bytes()
                        .all(|b| is_token_byte(b) && !b.is_ascii_uppercase())
            );
        }
        assert_eq!(SEC_WEBSOCKET_ACCEPT.as_str(), "sec-websocket-accept");
    }
}
