//! The error type of connection drivers.

use crate::h1::ProtoError;
use std::fmt;
use std::io;

/// Why a connection or exchange ended abnormally.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The peer broke the HTTP/1.1 rules.
    Proto(ProtoError),
    /// A deadline passed; the text says which step.
    Timeout(&'static str),
    /// The peer closed the connection in the middle of a message.
    UnexpectedEof,
    /// The body did not match its declared `Content-Length`.
    BodyLength,
    /// A body stream failed while being written.
    Body(crate::body::BoxError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "i/o error: {err}"),
            Self::Proto(err) => write!(f, "{err}"),
            Self::Timeout(what) => write!(f, "timed out {what}"),
            Self::UnexpectedEof => f.write_str("connection closed mid-message"),
            Self::BodyLength => f.write_str("body length does not match its Content-Length"),
            Self::Body(err) => write!(f, "body stream failed: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Proto(err) => Some(err),
            Self::Body(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<ProtoError> for Error {
    fn from(err: ProtoError) -> Self {
        Self::Proto(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    #[quench_test::test]
    fn displays_and_sources() {
        let io = Error::from(io::Error::other("boom"));
        assert!(io.to_string().contains("boom") && io.source().is_some());
        let proto = Error::from(ProtoError::UriTooLong);
        assert!(proto.to_string().contains("too long") && proto.source().is_some());
        let timeout = Error::Timeout("reading the head");
        assert_eq!(timeout.to_string(), "timed out reading the head");
        assert!(timeout.source().is_none());
        assert!(Error::UnexpectedEof.to_string().contains("mid-message"));
        assert!(Error::BodyLength.to_string().contains("Content-Length"));
        let body = Error::Body("stream died".into());
        assert!(body.to_string().contains("stream died") && body.source().is_some());
    }
}
