//! Errors: building a message is a different failure from delivering one.

use std::fmt;
use std::io;

/// Why a message could not be built. Nothing has touched the network yet.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    #[error("invalid email address {0:?}")]
    InvalidAddress(String),
    #[error("message has no From address")]
    MissingFrom,
    #[error("message has no recipients")]
    NoRecipients,
    #[error("message has neither a text nor an html body")]
    MissingBody,
    #[error("{0} contains a line break or control character")]
    HeaderInjection(&'static str),
    #[error("invalid header name {0:?}")]
    InvalidHeaderName(String),
    #[error("header {0:?} is set by quench-mail itself and cannot be overridden")]
    ReservedHeader(String),
}

/// An SMTP reply: status code plus the text lines that came with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    /// The RFC 3463 enhanced status (`5.7.1`) when the server sent one.
    pub enhanced: Option<String>,
    pub lines: Vec<String>,
}

impl Reply {
    /// The reply text, lines joined with a space.
    pub fn message(&self) -> String {
        self.lines.join(" ")
    }
}

impl fmt::Display for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(enhanced) = &self.enhanced {
            write!(f, " {enhanced}")?;
        }
        write!(f, " {}", self.message())
    }
}

/// Why a delivery attempt failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Io(#[from] io::Error),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    /// The server said something that is not SMTP, or too much of it.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The server refused a command. `stage` is which one (`"RCPT TO"`, ...).
    #[error("server rejected {stage}: {reply}")]
    Rejected { stage: &'static str, reply: Reply },
    #[error("authentication failed: {0}")]
    Auth(Reply),
    #[error("message is {size} bytes, the server accepts at most {limit}")]
    TooLarge { size: usize, limit: usize },
    /// The server cannot do what the configuration requires.
    #[error("{0}")]
    Unsupported(String),
    /// A configuration mistake caught before connecting.
    #[error("invalid configuration: {0}")]
    Config(String),
}

impl Error {
    /// Worth trying again later: network trouble, timeouts and 4xx replies.
    /// Never true for a 5xx refusal, a bad login, or a mistake in the message.
    pub fn is_transient(&self) -> bool {
        match self {
            Error::Io(_) | Error::Timeout(_) => true,
            Error::Rejected { reply, .. } => (400..500).contains(&reply.code),
            _ => false,
        }
    }

    /// The server's reply, for the variants that carry one.
    pub fn reply(&self) -> Option<&Reply> {
        match self {
            Error::Rejected { reply, .. } | Error::Auth(reply) => Some(reply),
            _ => None,
        }
    }
}
