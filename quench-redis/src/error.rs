//! Errors.

use std::fmt;
use std::io;

/// An error reply from the server (`-ERR ...`, `-MOVED ...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerError {
    /// The first word: `ERR`, `WRONGTYPE`, `MOVED`, `ASK`, `NOAUTH`, ...
    pub kind: String,
    pub message: String,
}

impl ServerError {
    /// Splits `KIND rest of the message`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let (kind, message) = text.split_once(' ').unwrap_or((text, ""));
        Self {
            kind: kind.to_string(),
            message: message.to_string(),
        }
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind, self.message)
    }
}

impl std::error::Error for ServerError {}

/// Why an operation failed.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The URL or options are unusable.
    Config(String),
    /// The peer sent something that is not RESP.
    Protocol(String),
    /// The server replied with an error.
    Server(ServerError),
    Tls(String),
    Timeout(&'static str),
    /// A reply could not be converted to the requested type.
    Type(String),
    /// A cluster command was redirected too many times, or the topology is unusable.
    Cluster(String),
    /// The connection was closed, or abandoned in the middle of a command.
    Closed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "i/o error: {err}"),
            Self::Config(why) => write!(f, "invalid configuration: {why}"),
            Self::Protocol(why) => write!(f, "protocol error: {why}"),
            Self::Server(err) => write!(f, "server error: {err}"),
            Self::Tls(why) => write!(f, "TLS error: {why}"),
            Self::Timeout(what) => write!(f, "timed out {what}"),
            Self::Type(why) => write!(f, "unexpected reply: {why}"),
            Self::Cluster(why) => write!(f, "cluster error: {why}"),
            Self::Closed => f.write_str("the connection is closed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Server(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[test]
    fn server_errors_split_kind_and_message() {
        let err =
            ServerError::parse("WRONGTYPE Operation against a key holding the wrong kind of value");
        assert_eq!(err.kind, "WRONGTYPE");
        assert!(err.message.starts_with("Operation"));
        assert_eq!(ServerError::parse("ERR").message, "");
        assert_eq!(
            err.to_string(),
            "WRONGTYPE Operation against a key holding the wrong kind of value"
        );
    }

    #[test]
    fn display_and_sources() {
        let all = [
            Error::Io(io::Error::other("x")),
            Error::Config("c".into()),
            Error::Protocol("p".into()),
            Error::Server(ServerError::parse("ERR x")),
            Error::Tls("t".into()),
            Error::Timeout("connecting"),
            Error::Type("int".into()),
            Error::Cluster("down".into()),
            Error::Closed,
        ];
        for err in &all {
            assert!(!err.to_string().is_empty());
        }
        assert!(
            all[0].source().is_some() && all[3].source().is_some() && all[8].source().is_none()
        );
        assert!(matches!(Error::from(io::Error::other("e")), Error::Io(_)));
    }
}
