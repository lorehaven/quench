//! The error a server function returns, and how it crosses the wire.
//!
//! Server errors map to HTTP statuses on the way out and back to the same variant on the way in,
//! so `match` in the browser sees what the server decided. The last two variants never travel:
//! they are what the browser reports when the call itself failed.

use std::fmt;

use quench_serde::{Deserialize, Serialize};

/// One thing wrong with one input.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

/// Why a server function did not produce a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    /// Not signed in, or the session ended (401).
    Unauthorized(String),
    /// Signed in, but not allowed (403).
    Forbidden(String),
    /// There is no such thing (404).
    NotFound(String),
    /// The input is not acceptable (422), with what is wrong with which field.
    Validation {
        message: String,
        fields: Vec<FieldError>,
    },
    /// Something broke on the server (500). The details stay in the server's log.
    Internal(String),
    /// Browser only: the request could not be made (offline, refused, blocked).
    Network(String),
    /// Browser only: the server answered, but not with what a server function sends.
    BadResponse(String),
}

/// The JSON a failing call responds with.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ErrorBody {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub fields: Vec<FieldError>,
}

/// What the browser is told for an internal error; the cause is logged on the server.
pub const INTERNAL_MESSAGE: &str = "internal server error";

impl ServerError {
    #[must_use]
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }

    #[must_use]
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    /// An invalid input with a single complaint about `field`.
    #[must_use]
    pub fn invalid(field: &str, message: &str) -> Self {
        Self::Validation {
            message: format!("{field}: {message}"),
            fields: vec![FieldError {
                field: field.to_string(),
                message: message.to_string(),
            }],
        }
    }

    /// An internal error with the cause to log. The browser only gets a generic message.
    #[must_use]
    pub fn internal(cause: impl fmt::Display) -> Self {
        Self::Internal(cause.to_string())
    }

    /// The HTTP status this error is sent with. The two browser-only variants have none of their
    /// own; a server that had to send them would answer 502.
    #[must_use]
    pub const fn status(&self) -> u16 {
        match self {
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::Validation { .. } => 422,
            Self::Internal(_) => 500,
            Self::Network(_) | Self::BadResponse(_) => 502,
        }
    }

    /// A short stable name, such as `not_found`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Unauthorized(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::NotFound(_) => "not_found",
            Self::Validation { .. } => "validation",
            Self::Internal(_) => "internal",
            Self::Network(_) => "network",
            Self::BadResponse(_) => "bad_response",
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::NotFound(m)
            | Self::Internal(m)
            | Self::Network(m)
            | Self::BadResponse(m) => m,
            Self::Validation { message, .. } => message,
        }
    }

    /// The complaints about individual fields (empty unless this is a validation error).
    #[must_use]
    pub fn fields(&self) -> &[FieldError] {
        match self {
            Self::Validation { fields, .. } => fields,
            _ => &[],
        }
    }

    /// The JSON body to send. An internal error's message is replaced by a generic one.
    #[must_use]
    pub fn to_body(&self) -> ErrorBody {
        ErrorBody {
            kind: self.kind().to_string(),
            message: match self {
                Self::Internal(_) => INTERNAL_MESSAGE.to_string(),
                other => other.message().to_string(),
            },
            fields: self.fields().to_vec(),
        }
    }

    /// The error a failed response stands for. A body that names a kind decides; anything else
    /// (a proxy's HTML page, an empty 502) falls back on the status.
    #[must_use]
    pub fn from_response(status: u16, body: &str) -> Self {
        let parsed: Option<ErrorBody> = quench_serde::json::from_str(body).ok();
        let message = |parsed: &Option<ErrorBody>| {
            parsed
                .as_ref()
                .map(|b| b.message.clone())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("the server answered {status}"))
        };
        let kind = parsed.as_ref().map_or("", |b| b.kind.as_str());
        match (kind, status) {
            ("validation", _) | (_, 422) => Self::Validation {
                message: message(&parsed),
                fields: parsed.map(|b| b.fields).unwrap_or_default(),
            },
            ("unauthorized", _) | (_, 401) => Self::Unauthorized(message(&parsed)),
            ("forbidden", _) | (_, 403) => Self::Forbidden(message(&parsed)),
            ("not_found", _) | (_, 404) => Self::NotFound(message(&parsed)),
            ("internal", _) | (_, 500) => Self::Internal(message(&parsed)),
            _ => Self::BadResponse(message(&parsed)),
        }
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for ServerError {}
