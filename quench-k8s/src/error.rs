//! What can go wrong talking to a cluster.

use std::fmt;

/// The API server's own account of a failure (a `Status` object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The HTTP status code.
    pub code: u16,
    /// The machine-readable reason (`NotFound`, `AlreadyExists`, `Forbidden`, ...), when the server gave one.
    pub reason: String,
    /// For people.
    pub message: String,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.code, self.reason, self.message)
    }
}

/// A failed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The configuration (kubeconfig, service account) is missing, unreadable or incomplete.
    Config(String),
    /// The server could not be reached, or the connection broke.
    Transport(String),
    /// The server answered with an error.
    Api(Status),
    /// The answer was not what the API promises.
    Decode(String),
    /// The configuration needs something this client does not do (credential plugins).
    Unsupported(String),
}

impl Error {
    /// The server's status, for an [`Error::Api`].
    #[must_use]
    pub const fn status(&self) -> Option<&Status> {
        match self {
            Self::Api(status) => Some(status),
            _ => None,
        }
    }

    fn is_code(&self, code: u16) -> bool {
        self.status().is_some_and(|s| s.code == code)
    }

    /// The object does not exist (404).
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        self.is_code(404)
    }

    /// The object already exists, or the update raced another (409).
    #[must_use]
    pub fn is_conflict(&self) -> bool {
        self.is_code(409)
    }

    /// The account may not do this (403).
    #[must_use]
    pub fn is_forbidden(&self) -> bool {
        self.is_code(403)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(m) => write!(f, "kubernetes configuration: {m}"),
            Self::Transport(m) => write!(f, "kubernetes request failed: {m}"),
            Self::Api(status) => write!(f, "kubernetes API error: {status}"),
            Self::Decode(m) => write!(f, "kubernetes response: {m}"),
            Self::Unsupported(m) => write!(f, "not supported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
