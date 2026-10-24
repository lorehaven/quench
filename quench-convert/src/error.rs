//! Errors of the readers.

use std::fmt;

/// A failure to read or write one of the formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    format: &'static str,
    message: String,
    /// Where the problem is: a byte offset for binary formats, a line (1-based) for text ones; 0 when unknown.
    position: usize,
}

impl Error {
    pub(crate) fn new(format: &'static str, message: impl Into<String>, position: usize) -> Self {
        Self {
            format,
            message: message.into(),
            position,
        }
    }

    /// Which format failed (`"csv"`, `"xml"`, `"axml"`, `"gguf"`).
    #[must_use]
    pub const fn format(&self) -> &'static str {
        self.format
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.position > 0 {
            write!(
                f,
                "{}: {} (at {})",
                self.format, self.message, self.position
            )
        } else {
            write!(f, "{}: {}", self.format, self.message)
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
