//! Errors: transport, protocol, authentication, and the server's own error reports.

use std::fmt;
use std::io;

/// A report the server sent in an `ErrorResponse` or `NoticeResponse`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerError {
    /// `ERROR`, `FATAL`, `PANIC` (errors) or `WARNING`, `NOTICE`, `DEBUG`, `INFO`, `LOG` (notices).
    pub severity: String,
    /// The five-character SQLSTATE, e.g. `23505` for a unique violation.
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    /// 1-based character offset into the query the error refers to.
    pub position: Option<u32>,
    pub schema: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
    pub constraint: Option<String>,
}

impl ServerError {
    /// Parses the field list of an error or notice message (`code byte, cstring` pairs, ending in 0).
    pub(crate) fn parse(mut payload: &[u8]) -> Self {
        let mut out = Self::default();
        while let Some((&code, rest)) = payload.split_first() {
            if code == 0 {
                break;
            }
            let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
            let value = String::from_utf8_lossy(&rest[..end]).into_owned();
            payload = rest.get(end + 1..).unwrap_or(&[]);
            match code {
                b'V' | b'S' if out.severity.is_empty() => out.severity = value,
                b'C' => out.code = value,
                b'M' => out.message = value,
                b'D' => out.detail = Some(value),
                b'H' => out.hint = Some(value),
                b'P' => out.position = value.parse().ok(),
                b's' => out.schema = Some(value),
                b't' => out.table = Some(value),
                b'c' => out.column = Some(value),
                b'n' => out.constraint = Some(value),
                _ => {}
            }
        }
        out
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} (SQLSTATE {})",
            self.severity, self.message, self.code
        )?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

/// Why an operation failed.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The connection string or options are unusable.
    Config(String),
    /// The server (or a test double) sent something that is not valid protocol.
    Protocol(String),
    /// Authentication failed or uses a method this client does not implement.
    Auth(String),
    Tls(String),
    /// The server reported an error for the statement.
    Server(Box<ServerError>),
    /// A deadline passed; the text says which step.
    Timeout(&'static str),
    /// The connection is closed, or an earlier cancelled operation left it in an unknown state.
    Closed,
    /// A value could not be converted to or from its Rust type.
    Decode(String),
    /// A query that must return exactly one row returned none or several.
    RowCount {
        expected: &'static str,
        got: usize,
    },
}

impl Error {
    /// The SQLSTATE of a server error.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Server(err) => Some(&err.code),
            _ => None,
        }
    }

    /// The server's report, if this is one.
    #[must_use]
    pub fn server(&self) -> Option<&ServerError> {
        match self {
            Self::Server(err) => Some(err),
            _ => None,
        }
    }

    /// `23505`: a unique constraint was violated.
    #[must_use]
    pub fn is_unique_violation(&self) -> bool {
        self.code() == Some("23505")
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "i/o error: {err}"),
            Self::Config(why) => write!(f, "invalid configuration: {why}"),
            Self::Protocol(why) => write!(f, "protocol error: {why}"),
            Self::Auth(why) => write!(f, "authentication failed: {why}"),
            Self::Tls(why) => write!(f, "TLS error: {why}"),
            Self::Server(err) => write!(f, "{err}"),
            Self::Timeout(what) => write!(f, "timed out {what}"),
            Self::Closed => f.write_str("the connection is closed or in an unknown state"),
            Self::Decode(why) => write!(f, "could not convert a value: {why}"),
            Self::RowCount { expected, got } => write!(f, "expected {expected} row, got {got}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Server(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl std::error::Error for ServerError {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<quench_serde::Error> for Error {
    fn from(err: quench_serde::Error) -> Self {
        Self::Decode(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;

    fn fields(parts: &[(u8, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (code, value) in parts {
            out.push(*code);
            out.extend_from_slice(value.as_bytes());
            out.push(0);
        }
        out.push(0);
        out
    }

    #[test]
    fn server_errors_parse_every_field() {
        let wire = fields(&[
            (b'S', "ERROR"),
            (b'V', "ERROR"),
            (b'C', "23505"),
            (
                b'M',
                "duplicate key value violates unique constraint \"users_pkey\"",
            ),
            (b'D', "Key (id)=(1) already exists."),
            (b'H', "pick another"),
            (b'P', "17"),
            (b's', "public"),
            (b't', "users"),
            (b'c', "id"),
            (b'n', "users_pkey"),
            (b'F', "nbtinsert.c"),
        ]);
        let err = ServerError::parse(&wire);
        assert_eq!(err.severity, "ERROR");
        assert_eq!(err.code, "23505");
        assert!(err.message.contains("duplicate key"));
        assert_eq!(err.detail.as_deref(), Some("Key (id)=(1) already exists."));
        assert_eq!(err.hint.as_deref(), Some("pick another"));
        assert_eq!(err.position, Some(17));
        assert_eq!(
            (
                err.schema.as_deref(),
                err.table.as_deref(),
                err.column.as_deref(),
                err.constraint.as_deref()
            ),
            (
                Some("public"),
                Some("users"),
                Some("id"),
                Some("users_pkey")
            )
        );
        assert!(
            err.to_string().contains("SQLSTATE 23505")
                && err.to_string().contains("already exists")
        );
    }

    #[test]
    fn malformed_field_lists_do_not_panic() {
        assert_eq!(ServerError::parse(b""), ServerError::default());
        let truncated = ServerError::parse(b"Mno terminator");
        assert_eq!(truncated.message, "no terminator");
        assert_eq!(ServerError::parse(b"C23505\0\0junk").code, "23505");
        assert_eq!(ServerError::parse(b"Pnotanumber\0\0").position, None);
        // The localized severity (`S`) is used only when the non-localized one (`V`) is absent.
        assert_eq!(
            ServerError::parse(b"SFEHLER\0VERROR\0\0").severity,
            "FEHLER"
        );
    }

    #[test]
    fn error_helpers_and_display() {
        let server = Error::Server(Box::new(ServerError {
            code: "23505".into(),
            message: "dup".into(),
            severity: "ERROR".into(),
            ..ServerError::default()
        }));
        assert_eq!(server.code(), Some("23505"));
        assert!(server.is_unique_violation());
        assert!(server.server().is_some());
        assert!(server.source().is_some());
        let other = Error::Closed;
        assert_eq!(
            (
                other.code(),
                other.server().is_none(),
                other.is_unique_violation()
            ),
            (None, true, false)
        );
        for err in [
            Error::Io(io::Error::other("boom")),
            Error::Config("bad".into()),
            Error::Protocol("odd".into()),
            Error::Auth("no".into()),
            Error::Tls("handshake".into()),
            Error::Timeout("connecting"),
            Error::Closed,
            Error::Decode("int".into()),
            Error::RowCount {
                expected: "one",
                got: 2,
            },
        ] {
            assert!(!err.to_string().is_empty());
        }
        assert!(Error::from(io::Error::other("x")).source().is_some());
        assert!(Error::Closed.source().is_none());
        assert!(matches!(
            Error::from(quench_serde::Error::new("bad")),
            Error::Decode(_)
        ));
    }
}
