//! Cookies (RFC 6265): parsing the request `Cookie` header and `Set-Cookie`
//! values, and building `Set-Cookie` values.
//!
//! The attribute order written by [`Display`](std::fmt::Display) is
//! `HttpOnly; SameSite; Secure; Path; Domain; Max-Age; Expires`, and parsing
//! is as lenient as a browser's cookie jar: unknown attributes and malformed
//! values for known ones are ignored, only a missing `=` or an empty name fail.

use quench_core::time::{Duration, Time};
use std::fmt;
use std::str::FromStr;

/// The `SameSite` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSite {
    Strict,
    Lax,
    None,
}

impl fmt::Display for SameSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Strict => "Strict",
            Self::Lax => "Lax",
            Self::None => "None",
        })
    }
}

/// Why a cookie string did not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// There is no `name=value` pair.
    MissingPair,
    EmptyName,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MissingPair => "the cookie is missing a name/value pair",
            Self::EmptyName => "the cookie's name is empty",
        })
    }
}

impl std::error::Error for ParseError {}

/// One cookie with its attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    name: String,
    value: String,
    path: Option<String>,
    domain: Option<String>,
    max_age: Option<Duration>,
    expires: Option<Time>,
    secure: Option<bool>,
    http_only: Option<bool>,
    same_site: Option<SameSite>,
}

impl Cookie {
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            path: None,
            domain: None,
            max_age: None,
            expires: None,
            secure: None,
            http_only: None,
            same_site: None,
        }
    }

    /// Starts a builder: `Cookie::build(("name", "value")).path("/").secure(true).build()`.
    #[must_use]
    pub fn build(pair: impl Into<Self>) -> CookieBuilder {
        CookieBuilder(pair.into())
    }

    /// Parses a `Set-Cookie` value or a single `name=value` pair.
    ///
    /// # Errors
    ///
    /// [`ParseError`] when there is no `=` in the first segment or the name is empty.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut parts = text.split(';');
        let pair = parts.next().unwrap_or("");
        let (name, value) = pair.split_once('=').ok_or(ParseError::MissingPair)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(ParseError::EmptyName);
        }
        let mut cookie = Self::new(name, value.trim());
        for attribute in parts {
            let (key, value) = attribute.split_once('=').map_or_else(
                || (attribute.trim(), None),
                |(k, v)| (k.trim(), Some(v.trim())),
            );
            match (key.to_ascii_lowercase().as_str(), value) {
                ("secure", _) => cookie.secure = Some(true),
                ("httponly", _) => cookie.http_only = Some(true),
                ("max-age", Some(v)) => {
                    let (negative, digits) =
                        v.strip_prefix('-').map_or((false, v), |rest| (true, rest));
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                        continue;
                    }
                    // RFC 6265 5.2.2: a negative value means "expire now".
                    cookie.max_age = Some(if negative {
                        Duration::seconds(0)
                    } else {
                        Duration::seconds(digits.parse().unwrap_or(i64::MAX))
                    });
                }
                // RFC 6265 5.2.3: a leading dot is ignored.
                ("domain", Some(v)) if !v.trim_start_matches('.').is_empty() => {
                    cookie.domain = Some(v.trim_start_matches('.').to_string());
                }
                ("path", Some(v)) => cookie.path = Some(v.to_string()),
                ("samesite", Some(v)) => {
                    cookie.same_site = match v.to_ascii_lowercase().as_str() {
                        "strict" => Some(SameSite::Strict),
                        "lax" => Some(SameSite::Lax),
                        "none" => Some(SameSite::None),
                        _ => cookie.same_site,
                    };
                }
                ("expires", Some(v)) => {
                    if let Some(time) = crate::date::parse(v) {
                        cookie.expires = Some(time);
                    }
                }
                _ => {}
            }
        }
        Ok(cookie)
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn domain(&self) -> Option<&str> {
        self.domain.as_deref()
    }

    #[must_use]
    pub const fn max_age(&self) -> Option<Duration> {
        self.max_age
    }

    #[must_use]
    pub const fn expires(&self) -> Option<Time> {
        self.expires
    }

    #[must_use]
    pub const fn secure(&self) -> Option<bool> {
        self.secure
    }

    #[must_use]
    pub const fn http_only(&self) -> Option<bool> {
        self.http_only
    }

    #[must_use]
    pub const fn same_site(&self) -> Option<SameSite> {
        self.same_site
    }
}

/// Collects attributes for a [`Cookie`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieBuilder(Cookie);

impl CookieBuilder {
    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.0.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn domain(mut self, domain: impl Into<String>) -> Self {
        let domain: String = domain.into();
        self.0.domain = Some(domain.trim_start_matches('.').to_string());
        self
    }

    #[must_use]
    pub const fn max_age(mut self, max_age: Duration) -> Self {
        self.0.max_age = Some(max_age);
        self
    }

    #[must_use]
    pub const fn expires(mut self, expires: Time) -> Self {
        self.0.expires = Some(expires);
        self
    }

    #[must_use]
    pub const fn secure(mut self, secure: bool) -> Self {
        self.0.secure = Some(secure);
        self
    }

    #[must_use]
    pub const fn http_only(mut self, http_only: bool) -> Self {
        self.0.http_only = Some(http_only);
        self
    }

    #[must_use]
    pub const fn same_site(mut self, same_site: SameSite) -> Self {
        self.0.same_site = Some(same_site);
        self
    }

    #[must_use]
    pub fn build(self) -> Cookie {
        self.0
    }
}

impl fmt::Display for CookieBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<CookieBuilder> for Cookie {
    fn from(builder: CookieBuilder) -> Self {
        builder.0
    }
}

impl fmt::Display for Cookie {
    /// Formats the cookie as a `Set-Cookie` value. Nothing is percent-encoded.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name, self.value)?;
        if self.http_only == Some(true) {
            f.write_str("; HttpOnly")?;
        }
        if let Some(same_site) = self.same_site {
            write!(f, "; SameSite={same_site}")?;
        }
        if self.secure == Some(true)
            || (self.secure.is_none() && self.same_site == Some(SameSite::None))
        {
            f.write_str("; Secure")?;
        }
        if let Some(path) = &self.path {
            write!(f, "; Path={path}")?;
        }
        if let Some(domain) = &self.domain {
            write!(f, "; Domain={domain}")?;
        }
        if let Some(max_age) = self.max_age {
            write!(f, "; Max-Age={}", max_age.num_seconds())?;
        }
        if let Some(expires) = &self.expires {
            write!(f, "; Expires={}", crate::date::format(expires))?;
        }
        Ok(())
    }
}

impl FromStr for Cookie {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, ParseError> {
        Self::parse(s)
    }
}

impl<N: Into<String>, V: Into<String>> From<(N, V)> for Cookie {
    fn from((name, value): (N, V)) -> Self {
        Self::new(name, value)
    }
}

/// The `name=value` pairs of a request `Cookie` header, skipping malformed ones.
pub fn parse_cookie_header(header: &str) -> impl Iterator<Item = (&str, &str)> {
    header.split(';').filter_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        let name = name.trim();
        (!name.is_empty()).then_some((name, value.trim()))
    })
}

/// The value of the first cookie called `name` in a request `Cookie` header.
#[must_use]
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    parse_cookie_header(header)
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_value() {
        let c = Cookie::parse("  session = abc123 ").unwrap();
        assert_eq!((c.name(), c.value()), ("session", "abc123"));
        assert_eq!(Cookie::parse("empty=").unwrap().value(), "");
        assert_eq!(Cookie::parse("eq=a=b").unwrap().value(), "a=b");
        assert_eq!("k=v".parse::<Cookie>().unwrap().name(), "k");
    }

    #[test]
    fn rejects_missing_pairs_and_empty_names() {
        assert_eq!(Cookie::parse("novalue"), Err(ParseError::MissingPair));
        assert_eq!(Cookie::parse(""), Err(ParseError::MissingPair));
        assert_eq!(Cookie::parse("=v"), Err(ParseError::EmptyName));
        assert_eq!(Cookie::parse("  =v"), Err(ParseError::EmptyName));
        assert!(ParseError::MissingPair.to_string().contains("name/value"));
        assert!(ParseError::EmptyName.to_string().contains("empty"));
    }

    #[test]
    fn parses_every_attribute() {
        let c = Cookie::parse(
            "id=1; Path=/app; Domain=example.com; Max-Age=3600; Secure; HttpOnly; SameSite=Strict; Expires=Wed, 21 Oct 2037 07:28:00 GMT",
        )
        .unwrap();
        assert_eq!(c.path(), Some("/app"));
        assert_eq!(c.domain(), Some("example.com"));
        assert_eq!(c.max_age(), Some(Duration::seconds(3600)));
        assert_eq!((c.secure(), c.http_only()), (Some(true), Some(true)));
        assert_eq!(c.same_site(), Some(SameSite::Strict));
        assert_eq!(c.expires().unwrap().timestamp(), 2_139_722_880);
    }

    #[test]
    fn attribute_names_are_case_insensitive_and_lenient() {
        let c = Cookie::parse("a=b; PATH=/x; sEcUrE; samesite=LAX; max-age=oops; domain=; unknown=1; flag; expires=garbage; samesite=bogus").unwrap();
        assert_eq!(c.path(), Some("/x"));
        assert_eq!(c.secure(), Some(true));
        assert_eq!(
            c.same_site(),
            Some(SameSite::Lax),
            "a bad value keeps the earlier one"
        );
        assert_eq!((c.max_age(), c.domain(), c.expires()), (None, None, None));
        assert_eq!(
            Cookie::parse("a=b; SameSite=none").unwrap().same_site(),
            Some(SameSite::None)
        );
        assert_eq!(
            Cookie::parse("a=b; Domain=.example.com").unwrap().domain(),
            Some("example.com")
        );
        assert_eq!(Cookie::parse("a=b; Domain=.").unwrap().domain(), None);
        assert_eq!(
            Cookie::build(("a", "b"))
                .domain(".example.com")
                .build()
                .domain(),
            Some("example.com")
        );
    }

    #[test]
    fn negative_and_huge_max_ages() {
        assert_eq!(
            Cookie::parse("a=b; Max-Age=-5").unwrap().max_age(),
            Some(Duration::seconds(0))
        );
        assert_eq!(
            Cookie::parse("a=b; Max-Age=99999999999999999999999")
                .unwrap()
                .max_age(),
            Some(Duration::seconds(i64::MAX))
        );
        assert_eq!(Cookie::parse("a=b; Max-Age=").unwrap().max_age(), None);
        assert_eq!(Cookie::parse("a=b; Max-Age=-").unwrap().max_age(), None);
    }

    #[test]
    fn builds_and_serialises_in_a_stable_order() {
        let builder = Cookie::build(("sid", "tok"))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(true)
            .domain("example.com")
            .max_age(Duration::minutes(5));
        assert_eq!(
            builder.to_string(),
            "sid=tok; HttpOnly; SameSite=Lax; Secure; Path=/; Domain=example.com; Max-Age=300"
        );
        let c = builder
            .expires(Time::from_ymd_hms(1994, 11, 6, 8, 49, 37).unwrap())
            .build();
        assert!(
            c.to_string()
                .ends_with("; Expires=Sun, 06 Nov 1994 08:49:37 GMT")
        );
        assert_eq!(Cookie::new("a", "b").to_string(), "a=b");
        assert_eq!(Cookie::build(("a", "b")).secure(false).to_string(), "a=b");
        assert_eq!(
            Cookie::build(("a", "b"))
                .http_only(false)
                .build()
                .to_string(),
            "a=b"
        );
        assert_eq!(
            Cookie::from(Cookie::build(("a", "b")).path("/")).path(),
            Some("/")
        );
    }

    #[test]
    fn same_site_none_implies_secure_unless_set() {
        assert_eq!(
            Cookie::build(("a", "b"))
                .same_site(SameSite::None)
                .to_string(),
            "a=b; SameSite=None; Secure"
        );
        assert_eq!(
            Cookie::build(("a", "b"))
                .same_site(SameSite::None)
                .secure(false)
                .to_string(),
            "a=b; SameSite=None"
        );
        assert_eq!(
            Cookie::build(("a", "b"))
                .same_site(SameSite::Strict)
                .to_string(),
            "a=b; SameSite=Strict"
        );
    }

    #[test]
    fn serialised_cookies_parse_back() {
        let original = Cookie::build(("k", "v"))
            .path("/p")
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(Duration::seconds(60))
            .domain("d.example")
            .build();
        assert_eq!(Cookie::parse(&original.to_string()).unwrap(), original);
    }

    #[test]
    fn request_cookie_headers() {
        let header = "a=1; b=two ; broken; =noname; c=3=3;a=shadowed";
        let pairs: Vec<_> = parse_cookie_header(header).collect();
        assert_eq!(
            pairs,
            [("a", "1"), ("b", "two"), ("c", "3=3"), ("a", "shadowed")]
        );
        assert_eq!(cookie_value(header, "a"), Some("1"));
        assert_eq!(cookie_value(header, "c"), Some("3=3"));
        assert_eq!(cookie_value(header, "missing"), None);
        assert_eq!(cookie_value("", "a"), None);
    }

    #[test]
    fn same_site_displays() {
        assert_eq!(SameSite::Strict.to_string(), "Strict");
        assert_eq!(SameSite::Lax.to_string(), "Lax");
        assert_eq!(SameSite::None.to_string(), "None");
    }
}
