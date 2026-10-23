//! Semantic versions and version requirements (`semver`'s role).
//!
//! Follows semver 2.0 for versions and cargo's rules for requirements: bare
//! `1.2` means `^1.2`, `~` and `^` as in cargo, `*` wildcards, comma-separated
//! comparators that must all match, and the pre-release rule (`1.3.0-rc.1`
//! only matches a requirement that names a pre-release of `1.3.0`).

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

/// Text that is not a version or requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn fail<T>(what: &str, text: &str) -> Result<T, ParseError> {
    Err(ParseError(format!("invalid {what}: {text:?}")))
}

/// `major.minor.patch[-pre-release][+build]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Dot-separated identifiers after `-`; empty for a release.
    pub pre: String,
    /// Dot-separated identifiers after `+`.
    pub build: String,
}

impl Version {
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: String::new(),
            build: String::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self, ParseError> {
        parse_version(text).map_or_else(|| fail("version", text), Ok)
    }
}

impl FromStr for Version {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Self, ParseError> {
        Self::parse(text)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build)?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(|| cmp_pre(&self.pre, &other.pre))
            .then_with(|| cmp_idents(&self.build, &other.build))
    }
}

/// A release outranks any of its pre-releases.
fn cmp_pre(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => cmp_idents(a, b),
    }
}

/// Identifier-wise: numbers numerically and below text; a prefix is smaller.
fn cmp_idents(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (
        a.split('.').filter(|s| !s.is_empty()),
        b.split('.').filter(|s| !s.is_empty()),
    );
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn number(text: &str) -> Option<u64> {
    let ok = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if ok { text.parse().ok() } else { None }
}

fn valid_idents(text: &str, forbid_leading_zero: bool) -> bool {
    text.split('.').all(|id| {
        !id.is_empty()
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !(forbid_leading_zero
                && id.len() > 1
                && id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Splits `core[-pre][+build]`.
fn split_extras(text: &str) -> Option<(&str, String, String)> {
    let (rest, build) = text.split_once('+').unwrap_or((text, ""));
    let (core, pre) = rest.split_once('-').unwrap_or((rest, ""));
    if (rest.contains('-') && !valid_idents(pre, true))
        || (text.contains('+') && !valid_idents(build, false))
    {
        return None;
    }
    Some((core, pre.to_string(), build.to_string()))
}

fn parse_version(text: &str) -> Option<Version> {
    let (core, pre, build) = split_extras(text)?;
    let mut parts = core.split('.');
    let (major, minor, patch) = (
        number(parts.next()?)?,
        number(parts.next()?)?,
        number(parts.next()?)?,
    );
    if parts.next().is_some() {
        return None;
    }
    Some(Version {
        major,
        minor,
        patch,
        pre,
        build,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Op {
    Exact,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    Tilde,
    Caret,
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Comparator {
    op: Op,
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    pre: String,
}

/// One or more comma-separated comparators that must all hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionReq {
    comparators: Vec<Comparator>,
}

impl VersionReq {
    /// Matches every release version.
    pub const STAR: Self = Self {
        comparators: Vec::new(),
    };

    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let text = text.trim();
        if matches!(text, "*" | "x" | "X") {
            return Ok(Self::STAR);
        }
        let comparators = text
            .split(',')
            .map(|part| parse_comparator(part.trim()))
            .collect::<Option<Vec<_>>>();
        comparators
            .map(|comparators| Self { comparators })
            .map_or_else(|| fail("version requirement", text), Ok)
    }

    #[must_use]
    pub fn matches(&self, version: &Version) -> bool {
        if !self.comparators.iter().all(|c| c.matches(version)) {
            return false;
        }
        version.pre.is_empty() || self.comparators.iter().any(|c| c.allows_pre(version))
    }
}

impl FromStr for VersionReq {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Self, ParseError> {
        Self::parse(text)
    }
}

fn parse_comparator(text: &str) -> Option<Comparator> {
    let (op, rest) = [
        (">=", Op::GreaterEq),
        ("<=", Op::LessEq),
        (">", Op::Greater),
        ("<", Op::Less),
        ("=", Op::Exact),
        ("~", Op::Tilde),
        ("^", Op::Caret),
    ]
    .iter()
    .find_map(|(prefix, op)| {
        text.strip_prefix(prefix)
            .map(|rest| (Some(*op), rest.trim_start()))
    })
    .unwrap_or((None, text));

    let (core, pre, build) = split_extras(rest)?;
    if !build.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major = number(parts.next()?)?;
    let wildcard = |part: &str| matches!(part, "*" | "x" | "X");
    let mut fields = [None, None];
    let mut saw_wildcard = false;
    for slot in &mut fields {
        match parts.next() {
            None => saw_wildcard = true,
            Some(part) if wildcard(part) => saw_wildcard = true,
            // A concrete field after a missing/wildcard one (`1.*.3`) is invalid.
            Some(part) if saw_wildcard => {
                let _ = part;
                return None;
            }
            Some(part) => *slot = Some(number(part)?),
        }
    }
    if parts.next().is_some() {
        return None;
    }
    let [minor, patch] = fields;
    // A pre-release only makes sense on a full `major.minor.patch`.
    if !pre.is_empty() && patch.is_none() {
        return None;
    }
    // `1.*` and `1.2.*` are wildcards when no operator is written.
    let explicit_wildcard = core.split('.').any(wildcard);
    let op = match op {
        Some(op) => op,
        None if explicit_wildcard => Op::Wildcard,
        None => Op::Caret,
    };
    Some(Comparator {
        op,
        major,
        minor,
        patch,
        pre,
    })
}

impl Comparator {
    fn matches(&self, v: &Version) -> bool {
        match self.op {
            Op::Exact | Op::Wildcard => self.exact(v),
            Op::Greater => self.greater(v),
            Op::GreaterEq => self.exact(v) || self.greater(v),
            Op::Less => self.less(v),
            Op::LessEq => self.exact(v) || self.less(v),
            Op::Tilde => self.tilde(v),
            Op::Caret => self.caret(v),
        }
    }

    fn exact(&self, v: &Version) -> bool {
        v.major == self.major
            && self.minor.is_none_or(|m| v.minor == m)
            && self.patch.is_none_or(|p| v.patch == p)
            && cmp_pre(&v.pre, &self.pre) == Ordering::Equal
    }

    fn greater(&self, v: &Version) -> bool {
        if v.major != self.major {
            return v.major > self.major;
        }
        let Some(minor) = self.minor else {
            return false;
        };
        if v.minor != minor {
            return v.minor > minor;
        }
        let Some(patch) = self.patch else {
            return false;
        };
        if v.patch != patch {
            return v.patch > patch;
        }
        cmp_pre(&v.pre, &self.pre) == Ordering::Greater
    }

    fn less(&self, v: &Version) -> bool {
        if v.major != self.major {
            return v.major < self.major;
        }
        let Some(minor) = self.minor else {
            return false;
        };
        if v.minor != minor {
            return v.minor < minor;
        }
        let Some(patch) = self.patch else {
            return false;
        };
        if v.patch != patch {
            return v.patch < patch;
        }
        cmp_pre(&v.pre, &self.pre) == Ordering::Less
    }

    fn tilde(&self, v: &Version) -> bool {
        if v.major != self.major || self.minor.is_some_and(|m| v.minor != m) {
            return false;
        }
        if let Some(patch) = self.patch
            && v.patch != patch
        {
            return v.patch > patch;
        }
        cmp_pre(&v.pre, &self.pre) != Ordering::Less
    }

    fn caret(&self, v: &Version) -> bool {
        if v.major != self.major {
            return false;
        }
        let Some(minor) = self.minor else { return true };
        let Some(patch) = self.patch else {
            return if self.major > 0 {
                v.minor >= minor
            } else {
                v.minor == minor
            };
        };
        if self.major > 0 {
            if v.minor != minor {
                return v.minor > minor;
            } else if v.patch != patch {
                return v.patch > patch;
            }
        } else if minor > 0 {
            if v.minor != minor {
                return false;
            } else if v.patch != patch {
                return v.patch > patch;
            }
        } else if v.minor != minor || v.patch != patch {
            return false;
        }
        cmp_pre(&v.pre, &self.pre) != Ordering::Less
    }

    /// A pre-release version only satisfies a requirement that names a
    /// pre-release of the very same `major.minor.patch`.
    fn allows_pre(&self, v: &Version) -> bool {
        self.major == v.major
            && self.minor == Some(v.minor)
            && self.patch == Some(v.patch)
            && !self.pre.is_empty()
    }
}

impl fmt::Display for VersionReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.comparators.is_empty() {
            return f.write_str("*");
        }
        for (i, c) in self.comparators.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Comparator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op = match self.op {
            Op::Exact => "=",
            Op::Greater => ">",
            Op::GreaterEq => ">=",
            Op::Less => "<",
            Op::LessEq => "<=",
            Op::Tilde => "~",
            Op::Caret => "^",
            Op::Wildcard => "",
        };
        write!(f, "{op}{}", self.major)?;
        match (self.minor, self.patch) {
            (Some(minor), Some(patch)) => write!(f, ".{minor}.{patch}")?,
            (Some(minor), None) => write!(
                f,
                ".{minor}{}",
                if self.op == Op::Wildcard { ".*" } else { "" }
            )?,
            (None, _) => {
                if self.op == Op::Wildcard {
                    f.write_str(".*")?;
                }
            }
        }
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn req(s: &str) -> VersionReq {
        VersionReq::parse(s).unwrap()
    }

    fn ok(r: &str, version: &str) -> bool {
        req(r).matches(&v(version))
    }

    #[test]
    fn parses_and_displays_versions() {
        assert_eq!(v("1.2.3"), Version::new(1, 2, 3));
        let full = v("1.2.3-alpha.1+build.5");
        assert_eq!(
            (full.pre.as_str(), full.build.as_str()),
            ("alpha.1", "build.5")
        );
        for text in ["0.0.0", "1.2.3", "1.2.3-rc.1", "1.2.3+b", "1.2.3-x-y.0+z"] {
            assert_eq!(v(text).to_string(), text);
        }
        assert_eq!("1.0.0".parse::<Version>().unwrap(), Version::new(1, 0, 0));
    }

    #[test]
    fn rejects_bad_versions() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "a.b.c",
            "1.2.3-",
            "1.2.3+",
            "1.2.3-01",
            "1.2.3-a..b",
            "1.2.3-a_b",
            "-1.2.3",
            "1.2.-3",
            " 1.2.3",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?}");
        }
        assert!(
            Version::parse("x")
                .unwrap_err()
                .to_string()
                .contains("invalid version")
        );
    }

    #[test]
    fn ordering_follows_semver() {
        // The ordering example from the semver 2.0 spec.
        let order = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "2.0.0",
        ];
        for pair in order.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert!(v("1.0.0+a") < v("1.0.0+b"));
        assert_eq!(v("1.0.0").cmp(&v("1.0.0")), Ordering::Equal);
        assert!(v("1.0.0-1") < v("1.0.0-a"));
        assert!(v("10.0.0") > v("9.9.9"));
    }

    #[test]
    fn caret_requirements() {
        assert!(ok("^1.2.3", "1.2.3") && ok("^1.2.3", "1.9.0") && !ok("^1.2.3", "2.0.0"));
        assert!(!ok("^1.2.3", "1.2.2"));
        assert!(ok("^0.2.3", "0.2.9") && !ok("^0.2.3", "0.3.0") && !ok("^0.2.3", "0.2.2"));
        assert!(ok("^0.0.3", "0.0.3") && !ok("^0.0.3", "0.0.4"));
        assert!(ok("^0.2", "0.2.7") && !ok("^0.2", "0.3.0") && !ok("^0.2", "0.1.9"));
        assert!(ok("^1", "1.9.9") && !ok("^1", "2.0.0"));
        assert!(ok("^0", "0.9.0") && !ok("^0", "1.0.0"));
        assert!(ok("^1.2", "1.3.0") && !ok("^1.2", "1.1.9"));
        // bare = caret
        assert!(ok("1.2.3", "1.5.0") && !ok("1.2.3", "2.0.0"));
        assert!(ok("0.1", "0.1.5") && !ok("0.1", "0.2.0"));
    }

    #[test]
    fn tilde_requirements() {
        assert!(ok("~1.2.3", "1.2.3") && ok("~1.2.3", "1.2.9") && !ok("~1.2.3", "1.3.0"));
        assert!(!ok("~1.2.3", "1.2.2"));
        assert!(ok("~1.2", "1.2.9") && !ok("~1.2", "1.3.0"));
        assert!(ok("~1", "1.9.0") && !ok("~1", "2.0.0"));
    }

    #[test]
    fn comparison_requirements() {
        assert!(ok(">=1.2.3", "1.2.3") && ok(">=1.2.3", "2.0.0") && !ok(">=1.2.3", "1.2.2"));
        assert!(
            ok(">1.2.3", "1.2.4")
                && !ok(">1.2.3", "1.2.3")
                && ok(">1.2.3", "1.3.0")
                && ok(">1.2.3", "2.0.0")
        );
        assert!(
            ok("<1.2.3", "1.2.2")
                && !ok("<1.2.3", "1.2.3")
                && ok("<1.2.3", "1.1.9")
                && ok("<1.2.3", "0.9.0")
        );
        assert!(ok("<=1.2.3", "1.2.3") && !ok("<=1.2.3", "1.2.4"));
        assert!(ok("=1.2.3", "1.2.3") && !ok("=1.2.3", "1.2.4"));
        assert!(ok("= 1.2.3", "1.2.3") && ok(">= 1.0.0", "1.0.0"));
        // partial versions
        assert!(ok(">1", "2.0.0") && !ok(">1", "1.9.9"));
        assert!(ok(">1.2", "1.3.0") && !ok(">1.2", "1.2.9"));
        assert!(ok("<1", "0.9.0") && !ok("<1", "1.0.0"));
        assert!(ok("<1.2", "1.1.9") && !ok("<1.2", "1.2.0"));
        assert!(ok("=1.2", "1.2.5") && !ok("=1.2", "1.3.0"));
        assert!(ok("=1", "1.7.0") && !ok("=1", "2.0.0"));
    }

    #[test]
    fn wildcards_and_star() {
        assert!(ok("*", "0.0.1") && ok("*", "99.0.0"));
        assert!(ok("1.*", "1.7.0") && !ok("1.*", "2.0.0"));
        assert!(ok("1.2.*", "1.2.9") && !ok("1.2.*", "1.3.0"));
        assert!(ok("1.x", "1.0.0") && ok("1.X.x", "1.0.0"));
        assert_eq!(req("x"), VersionReq::STAR);
        assert_eq!(req(" X "), VersionReq::STAR);
        assert!(VersionReq::STAR.matches(&v("3.2.1")));
        assert!(!VersionReq::STAR.matches(&v("3.2.1-rc.1")));
    }

    #[test]
    fn comma_separated_comparators_all_must_hold() {
        assert!(
            ok(">=1.2, <1.5", "1.4.0")
                && !ok(">=1.2, <1.5", "1.5.0")
                && !ok(">=1.2, <1.5", "1.1.0")
        );
        assert!(ok(" >=1.2.0 , <2.0.0 ", "1.9.9"));
    }

    #[test]
    fn pre_release_rule() {
        assert!(!ok("^1.2.3", "1.3.0-rc.1"));
        assert!(!ok(">=1.0.0", "2.0.0-rc.1"));
        assert!(ok("^1.3.0-rc.1", "1.3.0-rc.2"));
        assert!(ok("^1.3.0-rc.1", "1.3.0"));
        assert!(!ok("^1.3.0-rc.1", "1.3.0-beta"));
        assert!(!ok("^1.3.0-rc.1", "1.4.0-rc.1"));
        assert!(ok("=1.0.0-alpha", "1.0.0-alpha") && !ok("=1.0.0-alpha", "1.0.0-beta"));
        assert!(ok(">1.0.0-alpha", "1.0.0-beta") && ok("<1.0.0-beta", "1.0.0-alpha"));
        assert!(ok("~1.3.0-rc.1", "1.3.0-rc.2") && !ok("~1.3.0-rc.2", "1.3.0-rc.1"));
        assert!(ok("^0.3.0-a", "0.3.0-b") && ok("^0.0.3-a", "0.0.3-b") && ok("^0.3.0-a", "0.3.0"));
    }

    #[test]
    fn rejects_bad_requirements() {
        for bad in [
            "",
            "^",
            ">=",
            "1.*.3",
            "1.2.3.4",
            "^1.2.3+build",
            "1.2-rc.1",
            "~",
            "abc",
            "1.2.3,",
            ",1",
            "^^1",
            "=>1",
        ] {
            assert!(VersionReq::parse(bad).is_err(), "{bad:?}");
        }
        assert!(
            VersionReq::parse("xyz")
                .unwrap_err()
                .to_string()
                .contains("invalid version requirement")
        );
    }

    #[test]
    fn requirements_display_as_cargo_does() {
        for (input, shown) in [
            ("^0.2", "^0.2"),
            ("1.2", "^1.2"),
            ("~1.2.3", "~1.2.3"),
            (">=1.0.0, <2", ">=1.0.0, <2"),
            ("*", "*"),
            ("1.*", "1.*"),
            ("1.2.*", "1.2.*"),
            ("=1.2.3-rc.1", "=1.2.3-rc.1"),
            ("^1", "^1"),
        ] {
            assert_eq!(req(input).to_string(), shown, "{input}");
            assert_eq!(req(shown), req(input));
        }
        assert_eq!("^1".parse::<VersionReq>().unwrap(), req("^1"));
    }
}
