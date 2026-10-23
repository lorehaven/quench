//! Request targets and absolute URLs (RFC 3986, restricted to what HTTP needs).

use std::fmt;
use std::str::FromStr;

/// A parsed request target: `/path?query`, `scheme://authority/path?query`,
/// `host:port` (CONNECT) or `*` (OPTIONS).
///
/// Unlike `http::Uri`, accessors return `&str` rather than wrapper types, and
/// a fragment is rejected (it is never sent on the wire).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Uri {
    raw: String,
    scheme_end: Option<usize>,
    authority: Option<(usize, usize)>,
    path_start: usize,
    query_start: Option<usize>,
}

/// Why a string is not a [`Uri`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidUri {
    Empty,
    /// A space, control byte, non-ASCII byte or `#`.
    InvalidChar,
    InvalidScheme,
    InvalidAuthority,
    InvalidPort,
}

impl fmt::Display for InvalidUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty URI",
            Self::InvalidChar => "invalid character in URI",
            Self::InvalidScheme => "invalid URI scheme",
            Self::InvalidAuthority => "invalid URI authority",
            Self::InvalidPort => "invalid URI port",
        })
    }
}

impl std::error::Error for InvalidUri {}

impl Uri {
    /// Parses a request target or absolute URL.
    ///
    /// # Errors
    ///
    /// Returns an [`InvalidUri`] describing the first problem found.
    pub fn parse(src: &str) -> Result<Self, InvalidUri> {
        if src.is_empty() {
            return Err(InvalidUri::Empty);
        }
        if src.bytes().any(|b| b <= 0x20 || b >= 0x7f || b == b'#') {
            return Err(InvalidUri::InvalidChar);
        }
        if src == "*" || src.starts_with('/') {
            return Ok(Self::origin(src.to_string(), 0, None));
        }
        if let Some(sep) = src.find("://") {
            let scheme = &src[..sep];
            let mut chars = scheme.chars();
            let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
                && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
            if !valid {
                return Err(InvalidUri::InvalidScheme);
            }
            let auth_start = sep + 3;
            let auth_end = src[auth_start..]
                .find(['/', '?'])
                .map_or(src.len(), |i| auth_start + i);
            validate_authority(&src[auth_start..auth_end], true)?;
            let mut raw = src.to_string();
            if auth_end == raw.len() || raw.as_bytes()[auth_end] == b'?' {
                raw.insert(auth_end, '/');
            }
            let mut uri = Self::origin(raw, auth_end, Some((auth_start, auth_end)));
            uri.scheme_end = Some(sep);
            return Ok(uri);
        }
        validate_authority(src, false)?;
        Ok(Self {
            raw: src.to_string(),
            scheme_end: None,
            authority: Some((0, src.len())),
            path_start: src.len(),
            query_start: None,
        })
    }

    fn origin(raw: String, path_start: usize, authority: Option<(usize, usize)>) -> Self {
        let query_start = raw[path_start..].find('?').map(|i| path_start + i);
        Self {
            raw,
            scheme_end: None,
            authority,
            path_start,
            query_start,
        }
    }

    /// `http` or `https` (lowercase as written), when the target is absolute.
    #[must_use]
    pub fn scheme(&self) -> Option<&str> {
        self.scheme_end.map(|end| &self.raw[..end])
    }

    /// `userinfo@host:port` as written, when present.
    #[must_use]
    pub fn authority(&self) -> Option<&str> {
        self.authority.map(|(a, b)| &self.raw[a..b])
    }

    /// The host without userinfo or port; IPv6 literals keep their brackets.
    #[must_use]
    pub fn host(&self) -> Option<&str> {
        let authority = self.authority()?;
        let host_port = authority
            .rsplit_once('@')
            .map_or(authority, |(_, rest)| rest);
        Some(split_port(host_port).0)
    }

    #[must_use]
    pub fn port_u16(&self) -> Option<u16> {
        let authority = self.authority()?;
        let host_port = authority
            .rsplit_once('@')
            .map_or(authority, |(_, rest)| rest);
        split_port(host_port).1
    }

    /// The path; `/` for an absolute URL with none, empty for an authority-form target.
    #[must_use]
    pub fn path(&self) -> &str {
        let end = self.query_start.unwrap_or(self.raw.len());
        &self.raw[self.path_start..end]
    }

    #[must_use]
    pub fn query(&self) -> Option<&str> {
        self.query_start.map(|q| &self.raw[q + 1..])
    }

    /// `path?query`, what goes in an origin-form request line.
    #[must_use]
    pub fn path_and_query(&self) -> &str {
        match (self.path(), self.query_start) {
            ("", _) => "/",
            _ => &self.raw[self.path_start..],
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Resolves `reference` (a `Location` value) against this URL: absolute
    /// URLs replace it, `//host/..` keeps the scheme, `/path` keeps the
    /// authority, and a relative path replaces the last segment.
    ///
    /// # Errors
    ///
    /// Returns an [`InvalidUri`] when the result does not parse.
    pub fn resolve(&self, reference: &str) -> Result<Self, InvalidUri> {
        let reference = reference.split('#').next().unwrap_or("");
        if reference.contains("://") {
            return Self::parse(reference);
        }
        let scheme = self.scheme().unwrap_or("http");
        let authority = self.authority().unwrap_or("");
        if let Some(rest) = reference.strip_prefix("//") {
            return Self::parse(&format!("{scheme}://{rest}"));
        }
        let path = if reference.starts_with('/') {
            normalize_path(reference)
        } else if reference.starts_with('?') {
            format!("{}{reference}", self.path())
        } else {
            let dir = self.path().rfind('/').map_or("/", |i| &self.path()[..=i]);
            normalize_path(&format!("{dir}{reference}"))
        };
        Self::parse(&format!("{scheme}://{authority}{path}"))
    }
}

/// Collapses `.` and `..` segments of an absolute path, keeping any query.
fn normalize_path(path: &str) -> String {
    let (path, query) = path
        .split_once('?')
        .map_or((path, None), |(p, q)| (p, Some(q)));
    let mut out: Vec<&str> = Vec::new();
    let trailing = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut joined = format!("/{}", out.join("/"));
    if trailing && !joined.ends_with('/') {
        joined.push('/');
    }
    if let Some(query) = query {
        joined.push('?');
        joined.push_str(query);
    }
    joined
}

fn split_port(host_port: &str) -> (&str, Option<u16>) {
    let after_bracket = host_port.rfind(']').map_or(0, |i| i + 1);
    host_port[after_bracket..]
        .rfind(':')
        .map_or((host_port, None), |i| {
            let colon = after_bracket + i;
            (&host_port[..colon], host_port[colon + 1..].parse().ok())
        })
}

fn validate_authority(authority: &str, allow_userinfo: bool) -> Result<(), InvalidUri> {
    if authority.is_empty() {
        return Err(InvalidUri::InvalidAuthority);
    }
    let host_port = match authority.rsplit_once('@') {
        Some((_, rest)) if allow_userinfo => rest,
        Some(_) => return Err(InvalidUri::InvalidAuthority),
        None => authority,
    };
    let (host, _) = split_port(host_port);
    if host.is_empty() {
        return Err(InvalidUri::InvalidAuthority);
    }
    let port_text = &host_port[host.len()..];
    if let Some(port) = port_text.strip_prefix(':')
        && (port.is_empty() || port.parse::<u16>().is_err())
    {
        return Err(InvalidUri::InvalidPort);
    }
    if host.starts_with('[') {
        if !host.ends_with(']') {
            return Err(InvalidUri::InvalidAuthority);
        }
    } else if host.bytes().any(|b| {
        matches!(
            b,
            b'/' | b'?'
                | b'['
                | b']'
                | b'<'
                | b'>'
                | b'"'
                | b'\\'
                | b'^'
                | b'`'
                | b'{'
                | b'|'
                | b'}'
                | b'@'
        )
    }) {
        return Err(InvalidUri::InvalidAuthority);
    }
    Ok(())
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl fmt::Debug for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.raw, f)
    }
}

impl Default for Uri {
    fn default() -> Self {
        Self::origin("/".to_string(), 0, None)
    }
}

impl FromStr for Uri {
    type Err = InvalidUri;

    fn from_str(s: &str) -> Result<Self, InvalidUri> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Uri {
    type Error = InvalidUri;

    fn try_from(s: &str) -> Result<Self, InvalidUri> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Uri {
    type Error = InvalidUri;

    fn try_from(s: String) -> Result<Self, InvalidUri> {
        Self::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> Uri {
        Uri::parse(text).unwrap()
    }

    #[test]
    fn origin_form() {
        let u = uri("/a/b?x=1&y=2");
        assert_eq!((u.path(), u.query()), ("/a/b", Some("x=1&y=2")));
        assert_eq!(
            (u.scheme(), u.authority(), u.host(), u.port_u16()),
            (None, None, None, None)
        );
        assert_eq!(u.path_and_query(), "/a/b?x=1&y=2");
        assert_eq!(u.to_string(), "/a/b?x=1&y=2");
        assert_eq!(uri("/").query(), None);
        assert_eq!(uri("/p?").query(), Some(""));
        assert_eq!(uri("*").path(), "*");
        assert_eq!(Uri::default().path(), "/");
        assert_eq!(format!("{:?}", uri("/x")), "\"/x\"");
    }

    #[test]
    fn absolute_form() {
        let u = uri("https://user:pw@Example.com:8443/path/x?q=1");
        assert_eq!(u.scheme(), Some("https"));
        assert_eq!(u.authority(), Some("user:pw@Example.com:8443"));
        assert_eq!(u.host(), Some("Example.com"));
        assert_eq!(u.port_u16(), Some(8443));
        assert_eq!((u.path(), u.query()), ("/path/x", Some("q=1")));
        assert_eq!(u.as_str(), "https://user:pw@Example.com:8443/path/x?q=1");
    }

    #[test]
    fn absolute_form_without_a_path_gets_a_slash() {
        for (raw, normalized) in [
            ("http://h", "http://h/"),
            ("http://h?x=1", "http://h/?x=1"),
            ("http://h:80", "http://h:80/"),
        ] {
            let u = uri(raw);
            assert_eq!(u.as_str(), normalized);
            assert_eq!(u.path(), "/");
        }
        assert_eq!(uri("http://h").port_u16(), None);
        assert_eq!(uri("http://h?x=1").query(), Some("x=1"));
    }

    #[test]
    fn authority_form_and_ipv6() {
        let u = uri("example.com:443");
        assert_eq!(
            (u.host(), u.port_u16(), u.path(), u.scheme()),
            (Some("example.com"), Some(443), "", None)
        );
        assert_eq!(u.path_and_query(), "/");
        let v6 = uri("http://[::1]:3000/x");
        assert_eq!((v6.host(), v6.port_u16()), (Some("[::1]"), Some(3000)));
        assert_eq!(uri("http://[::1]/").port_u16(), None);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Uri::parse(""), Err(InvalidUri::Empty));
        for bad in ["/a b", "/a\r\n", "/caf\u{e9}", "/x#frag", "/\x7f"] {
            assert_eq!(Uri::parse(bad), Err(InvalidUri::InvalidChar), "{bad:?}");
        }
        for bad in ["1http://h/", "ht tp://h/", "://h/", "ht!tp://h/"] {
            assert!(
                matches!(
                    Uri::parse(bad),
                    Err(InvalidUri::InvalidScheme | InvalidUri::InvalidChar)
                ),
                "{bad:?}"
            );
        }
        for bad in [
            "http:///x",
            "http://[::1/x",
            "http://h<>/x",
            "host@name",
            "a/b",
            ":80",
        ] {
            assert!(Uri::parse(bad).is_err(), "{bad:?}");
        }
        assert_eq!(Uri::parse("http://h:99999/"), Err(InvalidUri::InvalidPort));
        assert_eq!(Uri::parse("http://h:/"), Err(InvalidUri::InvalidPort));
        assert_eq!(Uri::parse("http://h:ab/"), Err(InvalidUri::InvalidPort));
        for err in [
            InvalidUri::Empty,
            InvalidUri::InvalidChar,
            InvalidUri::InvalidScheme,
            InvalidUri::InvalidAuthority,
            InvalidUri::InvalidPort,
        ] {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn conversions() {
        assert_eq!("/x".parse::<Uri>().unwrap().path(), "/x");
        assert_eq!(Uri::try_from("/y").unwrap().path(), "/y");
        assert_eq!(Uri::try_from(String::from("/z")).unwrap().path(), "/z");
        assert!(Uri::try_from(String::new()).is_err());
    }

    #[test]
    fn resolves_redirect_targets() {
        let base = uri("http://h:8080/a/b/c?old=1");
        assert_eq!(
            base.resolve("http://other/x").unwrap().as_str(),
            "http://other/x"
        );
        assert_eq!(base.resolve("//cdn/y").unwrap().as_str(), "http://cdn/y");
        assert_eq!(
            base.resolve("/abs?z=1").unwrap().as_str(),
            "http://h:8080/abs?z=1"
        );
        assert_eq!(
            base.resolve("rel").unwrap().as_str(),
            "http://h:8080/a/b/rel"
        );
        assert_eq!(
            base.resolve("../up").unwrap().as_str(),
            "http://h:8080/a/up"
        );
        assert_eq!(
            base.resolve("./same/").unwrap().as_str(),
            "http://h:8080/a/b/same/"
        );
        assert_eq!(
            base.resolve("?new=2").unwrap().as_str(),
            "http://h:8080/a/b/c?new=2"
        );
        assert_eq!(base.resolve("/p#frag").unwrap().as_str(), "http://h:8080/p");
        assert_eq!(
            base.resolve("../../../../way/up").unwrap().as_str(),
            "http://h:8080/way/up"
        );
        assert_eq!(base.resolve("..").unwrap().as_str(), "http://h:8080/a/");
        assert!(base.resolve("http://bad host/").is_err());
        assert!(
            uri("/relative").resolve("x").is_err(),
            "a relative base has no authority to resolve against"
        );
    }
}
