//! HTTP request methods (RFC 9110 section 9).

use std::fmt;
use std::str::FromStr;

/// An HTTP method: one of the nine standard ones, or an extension token.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Method(Inner);

#[derive(Clone, PartialEq, Eq, Hash)]
enum Inner {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
    Extension(Box<str>),
}

/// The bytes given to [`Method::from_bytes`] are not a valid method token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidMethod;

impl fmt::Display for InvalidMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid HTTP method")
    }
}

impl std::error::Error for InvalidMethod {}

impl Method {
    pub const GET: Self = Self(Inner::Get);
    pub const HEAD: Self = Self(Inner::Head);
    pub const POST: Self = Self(Inner::Post);
    pub const PUT: Self = Self(Inner::Put);
    pub const DELETE: Self = Self(Inner::Delete);
    pub const CONNECT: Self = Self(Inner::Connect);
    pub const OPTIONS: Self = Self(Inner::Options);
    pub const TRACE: Self = Self(Inner::Trace);
    pub const PATCH: Self = Self(Inner::Patch);

    /// Parses a method token. Methods are case-sensitive.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidMethod`] for an empty slice or a non-token byte.
    pub fn from_bytes(src: &[u8]) -> Result<Self, InvalidMethod> {
        Ok(match src {
            b"GET" => Self::GET,
            b"HEAD" => Self::HEAD,
            b"POST" => Self::POST,
            b"PUT" => Self::PUT,
            b"DELETE" => Self::DELETE,
            b"CONNECT" => Self::CONNECT,
            b"OPTIONS" => Self::OPTIONS,
            b"TRACE" => Self::TRACE,
            b"PATCH" => Self::PATCH,
            _ => {
                if src.is_empty() || !src.iter().all(|&b| crate::header::is_token_byte(b)) {
                    return Err(InvalidMethod);
                }
                let text = std::str::from_utf8(src).map_err(|_| InvalidMethod)?;
                Self(Inner::Extension(text.into()))
            }
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Inner::Get => "GET",
            Inner::Head => "HEAD",
            Inner::Post => "POST",
            Inner::Put => "PUT",
            Inner::Delete => "DELETE",
            Inner::Connect => "CONNECT",
            Inner::Options => "OPTIONS",
            Inner::Trace => "TRACE",
            Inner::Patch => "PATCH",
            Inner::Extension(name) => name,
        }
    }

    /// Methods that promise not to change server state (RFC 9110 9.2.1).
    #[must_use]
    pub const fn is_safe(&self) -> bool {
        matches!(
            self.0,
            Inner::Get | Inner::Head | Inner::Options | Inner::Trace
        )
    }

    /// Methods whose repeated request has the same effect as one (RFC 9110 9.2.2).
    #[must_use]
    pub const fn is_idempotent(&self) -> bool {
        self.is_safe() || matches!(self.0, Inner::Put | Inner::Delete)
    }
}

impl Default for Method {
    fn default() -> Self {
        Self::GET
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = InvalidMethod;

    fn from_str(s: &str) -> Result<Self, InvalidMethod> {
        Self::from_bytes(s.as_bytes())
    }
}

impl TryFrom<&str> for Method {
    type Error = InvalidMethod;

    fn try_from(s: &str) -> Result<Self, InvalidMethod> {
        Self::from_bytes(s.as_bytes())
    }
}

impl AsRef<str> for Method {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<str> for Method {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Method {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<Method> for &Method {
    fn eq(&self, other: &Method) -> bool {
        *self == other
    }
}

impl PartialEq<&Self> for Method {
    fn eq(&self, other: &&Self) -> bool {
        self == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_methods_round_trip() {
        for name in [
            "GET", "HEAD", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "TRACE", "PATCH",
        ] {
            let method: Method = name.parse().unwrap();
            assert_eq!(method.as_str(), name);
            assert_eq!(method.to_string(), name);
            assert_eq!(format!("{method:?}"), name);
        }
        assert_eq!(Method::from_bytes(b"GET").unwrap(), Method::GET);
        assert_eq!(Method::default(), Method::GET);
    }

    #[test]
    fn extension_methods_are_tokens() {
        let m = Method::from_bytes(b"PURGE").unwrap();
        assert_eq!(m.as_str(), "PURGE");
        assert!(!m.is_safe() && !m.is_idempotent());
        assert_ne!(m, Method::GET);
        assert_eq!(Method::try_from("M-SEARCH").unwrap().as_str(), "M-SEARCH");
    }

    #[test]
    fn invalid_methods_are_rejected() {
        for bad in [&b""[..], b"GE T", b"G\xc3\xa9T", b"GET\r\n", b"a/b"] {
            assert_eq!(Method::from_bytes(bad), Err(InvalidMethod), "{bad:?}");
        }
        assert!(InvalidMethod.to_string().contains("invalid"));
        assert!("lower case".parse::<Method>().is_err());
    }

    #[test]
    fn safety_and_idempotency() {
        assert!(
            Method::GET.is_safe()
                && Method::HEAD.is_safe()
                && Method::OPTIONS.is_safe()
                && Method::TRACE.is_safe()
        );
        assert!(!Method::POST.is_safe() && !Method::PUT.is_safe());
        assert!(
            Method::PUT.is_idempotent()
                && Method::DELETE.is_idempotent()
                && Method::GET.is_idempotent()
        );
        assert!(!Method::POST.is_idempotent() && !Method::PATCH.is_idempotent());
    }

    #[test]
    fn comparisons_with_strings_and_references() {
        assert_eq!(Method::GET, "GET");
        assert!(Method::GET.eq("GET"));
        let m = Method::POST;
        assert_eq!(&m, Method::POST);
        assert_eq!(Method::POST, &m);
        assert_eq!(m.as_ref(), "POST");
    }
}
