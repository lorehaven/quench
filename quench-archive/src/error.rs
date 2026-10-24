//! What can go wrong reading or writing an archive.

use std::fmt;

/// A failure to read or write an archive or a compressed stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The input ends before the structure it describes.
    Truncated,
    /// The input is not valid for its format.
    Corrupt(String),
    /// A valid input that uses something this crate does not implement (a zstd dictionary, zip64, ...).
    Unsupported(String),
    /// The output would be larger than the limit the caller set.
    Limit(String),
    /// A checksum stored in the input does not match its content.
    Checksum(String),
    /// The underlying reader or writer failed.
    Io(String),
}

impl Error {
    pub(crate) fn corrupt(message: impl Into<String>) -> Self {
        Self::Corrupt(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("unexpected end of data"),
            Self::Corrupt(m) => write!(f, "corrupt data: {m}"),
            Self::Unsupported(m) => write!(f, "unsupported: {m}"),
            Self::Limit(m) => write!(f, "limit exceeded: {m}"),
            Self::Checksum(m) => write!(f, "checksum mismatch: {m}"),
            Self::Io(m) => write!(f, "i/o error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// `Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
