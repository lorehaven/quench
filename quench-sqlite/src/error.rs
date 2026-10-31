//! Why a database file could not be read.

use std::fmt;
use std::io;

/// The result of every operation in this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// What went wrong.
#[derive(Debug)]
pub enum Error {
    /// The operating system refused a read or a write.
    Io(io::Error),
    /// The file is not a SQLite 3 database.
    NotADatabase(String),
    /// The file is a database, but a page or a record in it is malformed. `page` is 0 when no page applies.
    Corrupt { page: u32, reason: String },
    /// The SQL text is not valid; `pos` is the byte offset where parsing stopped.
    Syntax { pos: usize, message: String },
    /// A constraint (`UNIQUE`, `NOT NULL`, `CHECK`) was broken; the text says which.
    Constraint(String),
    /// The key (a rowid or an index entry) is already there.
    KeyExists,
    /// Another handle holds the file for writing.
    Busy,
    /// A write was attempted on a database opened for reading, or outside a transaction.
    ReadOnly(&'static str),
    /// A valid database that uses something this crate does not do (UTF-16 text, an unknown format number, ...).
    Unsupported(String),
}

impl Error {
    pub(crate) fn corrupt(page: u32, reason: impl Into<String>) -> Self {
        Self::Corrupt {
            page,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "i/o error: {e}"),
            Self::NotADatabase(why) => write!(f, "not a SQLite database: {why}"),
            Self::Corrupt { page: 0, reason } => write!(f, "corrupt database: {reason}"),
            Self::Corrupt { page, reason } => write!(f, "corrupt database, page {page}: {reason}"),
            Self::Syntax { pos, message } => write!(f, "SQL syntax error at byte {pos}: {message}"),
            Self::Constraint(what) => write!(f, "{what}"),
            Self::KeyExists => write!(f, "the key already exists"),
            Self::Busy => write!(f, "the database is locked by another writer"),
            Self::ReadOnly(what) => write!(f, "read-only: {what}"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
