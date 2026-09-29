//! Email addresses and mailboxes (`Name <user@host>`), validated on the way in.
//!
//! Deliberately stricter than RFC 5322: ASCII only (send an internationalised
//! domain as punycode), no quoted local parts, no comments, no domain literals.
//! Every address that gets past here is safe to put in an SMTP command and a
//! header without further escaping.

use crate::error::BuildError;
use std::fmt;
use std::str::FromStr;

/// `local@domain`, validated.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    full: String,
    at: usize,
}

impl Address {
    pub fn new(local: &str, domain: &str) -> Result<Self, BuildError> {
        format!("{local}@{domain}").parse()
    }

    pub fn as_str(&self) -> &str {
        &self.full
    }

    pub fn local(&self) -> &str {
        &self.full[..self.at]
    }

    pub fn domain(&self) -> &str {
        &self.full[self.at + 1..]
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.full)
    }
}

impl FromStr for Address {
    type Err = BuildError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || BuildError::InvalidAddress(s.to_string());
        let (local, domain) = s.rsplit_once('@').ok_or_else(invalid)?;
        if s.len() > 254 || !valid_local(local) || !valid_domain(domain) {
            return Err(invalid());
        }
        Ok(Self {
            full: s.to_string(),
            at: local.len(),
        })
    }
}

fn valid_local(local: &str) -> bool {
    const SPECIALS: &str = "!#$%&'*+/=?^_`{|}~-";
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || SPECIALS.contains(c))
}

fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// An address with an optional display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    pub name: Option<String>,
    pub address: Address,
}

impl Mailbox {
    pub fn new(name: Option<String>, address: Address) -> Self {
        Self { name, address }
    }
}

impl From<Address> for Mailbox {
    fn from(address: Address) -> Self {
        Self {
            name: None,
            address,
        }
    }
}

impl fmt::Display for Mailbox {
    /// Unencoded, for logs - the wire form is built by the message formatter.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(name) => write!(f, "{name} <{}>", self.address),
            None => write!(f, "{}", self.address),
        }
    }
}

impl FromStr for Mailbox {
    type Err = BuildError;

    /// Accepts `user@host`, `<user@host>`, `Name <user@host>` and
    /// `"Quoted, Name" <user@host>`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let Some(open) = s.rfind('<') else {
            return Ok(Self::from(s.parse::<Address>()?));
        };
        let Some(inner) = s[open + 1..].strip_suffix('>') else {
            return Err(BuildError::InvalidAddress(s.to_string()));
        };
        let address: Address = inner.trim().parse()?;

        let raw = s[..open].trim();
        let name = unquote(raw).ok_or_else(|| BuildError::InvalidAddress(s.to_string()))?;
        if name.chars().any(char::is_control) {
            return Err(BuildError::HeaderInjection("display name"));
        }
        Ok(Self {
            name: (!name.is_empty()).then_some(name),
            address,
        })
    }
}

/// `"a \"b\""` -> `a "b"`; anything not quoted passes through. `None` when a
/// quote is opened and never closed.
fn unquote(raw: &str) -> Option<String> {
    let Some(rest) = raw.strip_prefix('"') else {
        return Some(raw.to_string());
    };
    let mut out = String::new();
    let mut chars = rest.chars();
    loop {
        match chars.next()? {
            '\\' => out.push(chars.next()?),
            '"' => return chars.as_str().trim().is_empty().then_some(out),
            c => out.push(c),
        }
    }
}
