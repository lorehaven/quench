//! What can go wrong reading a PDF.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The data is not a PDF or its structure is damaged beyond reading.
    Malformed(String),
    /// A valid PDF that uses something this reader does not implement (encryption, ...).
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "malformed PDF: {m}"),
            Self::Unsupported(m) => write!(f, "unsupported PDF: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
