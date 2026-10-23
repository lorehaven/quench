//! HTTP protocol versions.

use std::fmt;

/// The protocol version in a request or status line.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(Inner);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Inner {
    Http09,
    Http10,
    Http11,
    H2,
    H3,
}

impl Version {
    pub const HTTP_09: Self = Self(Inner::Http09);
    pub const HTTP_10: Self = Self(Inner::Http10);
    pub const HTTP_11: Self = Self(Inner::Http11);
    pub const HTTP_2: Self = Self(Inner::H2);
    pub const HTTP_3: Self = Self(Inner::H3);

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self.0 {
            Inner::Http09 => "HTTP/0.9",
            Inner::Http10 => "HTTP/1.0",
            Inner::Http11 => "HTTP/1.1",
            Inner::H2 => "HTTP/2.0",
            Inner::H3 => "HTTP/3.0",
        }
    }
}

impl Default for Version {
    fn default() -> Self {
        Self::HTTP_11
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_and_orders() {
        assert_eq!(Version::HTTP_11.to_string(), "HTTP/1.1");
        assert_eq!(format!("{:?}", Version::HTTP_10), "HTTP/1.0");
        assert_eq!(Version::default(), Version::HTTP_11);
        assert!(Version::HTTP_09 < Version::HTTP_10 && Version::HTTP_10 < Version::HTTP_11);
        assert_eq!(Version::HTTP_2.as_str(), "HTTP/2.0");
        assert_eq!(Version::HTTP_3.as_str(), "HTTP/3.0");
        assert_eq!(Version::HTTP_09.as_str(), "HTTP/0.9");
    }
}
