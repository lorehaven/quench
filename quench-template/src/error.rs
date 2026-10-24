//! Template errors.

use std::fmt;

/// What kind of failure an [`Error`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The template text is not valid.
    Syntax,
    /// A value that does not exist was used where one is needed (an attribute of an undefined value).
    Undefined,
    /// An operation on values that do not support it.
    InvalidOperation,
    /// An imported or included template could not be found.
    TemplateNotFound,
    /// A filter, test or function that is not registered.
    UnknownName,
    /// A filter, test, function or formatter reported a failure.
    Custom,
}

/// A failure to parse or render a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    template: Option<String>,
    line: usize,
}

impl Error {
    #[must_use]
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            template: None,
            line: 0,
        }
    }

    pub(crate) fn syntax(message: impl Into<String>, line: usize) -> Self {
        Self {
            line,
            ..Self::new(ErrorKind::Syntax, message)
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The failure without the template name and line.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The template the error happened in, when known.
    #[must_use]
    pub fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    /// The line in the template (1-based), 0 when unknown.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Adds where the error happened, unless it already says.
    pub(crate) fn at(mut self, template: &str, line: usize) -> Self {
        if self.template.is_none() {
            self.template = Some(template.to_string());
        }
        if self.line == 0 {
            self.line = line;
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            ErrorKind::Syntax => "syntax error",
            ErrorKind::Undefined => "undefined value",
            ErrorKind::InvalidOperation => "invalid operation",
            ErrorKind::TemplateNotFound => "template not found",
            ErrorKind::UnknownName => "unknown name",
            ErrorKind::Custom => "error",
        };
        write!(f, "{what}: {}", self.message)?;
        match (&self.template, self.line) {
            (Some(name), 0) => write!(f, " (in {name})"),
            (Some(name), line) => write!(f, " (in {name}:{line})"),
            (None, 0) => Ok(()),
            (None, line) => write!(f, " (line {line})"),
        }
    }
}

impl std::error::Error for Error {}

/// `Result` with this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
