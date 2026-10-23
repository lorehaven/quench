//! Pattern errors.

use std::fmt;

/// A pattern that cannot be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    /// Character offset into the pattern, when the problem has a place.
    offset: Option<usize>,
}

impl Error {
    pub(crate) fn syntax(message: &str, offset: usize) -> Self {
        Self {
            message: message.to_string(),
            offset: Some(offset),
        }
    }

    pub(crate) fn too_big() -> Self {
        Self {
            message: "the compiled pattern would be too large".to_string(),
            offset: None,
        }
    }

    /// What is wrong, without the position.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Where in the pattern (a character offset), if it has a place.
    #[must_use]
    pub const fn offset(&self) -> Option<usize> {
        self.offset
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(f, "regex parse error at {offset}: {}", self.message),
            None => write!(f, "regex error: {}", self.message),
        }
    }
}

impl std::error::Error for Error {}
