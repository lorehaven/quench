//! Why parsing stopped.

use std::fmt;

/// What went wrong (or why parsing ended early).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// `-h` / `--help` / the `help` subcommand: not a failure, the text is the help.
    DisplayHelp,
    /// `-V` / `--version`: not a failure, the text is the version line.
    DisplayVersion,
    /// A flag or value nothing accepts.
    UnknownArgument,
    /// A word where a subcommand was expected, and it is none of them.
    UnknownSubcommand,
    /// A required argument was left out.
    MissingRequired,
    /// A subcommand is required and none was given.
    MissingSubcommand,
    /// An option that takes a value got none.
    MissingValue,
    /// A value that does not parse, or is not one of the allowed spellings.
    InvalidValue,
    /// Two arguments that exclude each other were both given.
    ArgumentConflict,
    /// A value was given to a switch (`--flag=value`).
    UnexpectedValue,
    /// A command-line word that is not valid UTF-8.
    InvalidUtf8,
}

/// A failed (or short-circuited) parse.
#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    usage: Option<String>,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            usage: None,
        }
    }

    pub(crate) fn with_usage(mut self, usage: String) -> Self {
        self.usage = Some(usage);
        self
    }

    /// An error for a value that did not parse, for use by [`FromArg`](crate::FromArg) implementations and the
    /// derive.
    #[must_use]
    pub fn invalid_value(arg: &str, value: &str, reason: &str) -> Self {
        Self::new(
            ErrorKind::InvalidValue,
            format!("invalid value '{value}' for '{arg}': {reason}"),
        )
    }

    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Whether this is `--help` or `--version` rather than a mistake.
    #[must_use]
    pub const fn is_display(&self) -> bool {
        matches!(
            self.kind,
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
        )
    }

    /// The message alone, without the `error:` prefix or the usage.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Everything to print: the help or version text as it is, or `error: ..` with the usage and a pointer to
    /// `--help`.
    #[must_use]
    pub fn render(&self) -> String {
        if self.is_display() {
            return self.message.clone();
        }
        let mut out = format!("error: {}\n", self.message);
        if let Some(usage) = &self.usage {
            out.push('\n');
            out.push_str(usage);
            out.push('\n');
        }
        out.push_str("\nFor more information, try '--help'.\n");
        out
    }

    /// Prints [`render`](Self::render) (help and version to standard output, mistakes to standard error).
    pub fn print(&self) {
        if self.is_display() {
            print!("{}", self.render());
        } else {
            eprint!("{}", self.render());
        }
    }

    /// Prints and exits: status 0 for help and version, 2 for a mistake.
    pub fn exit(&self) -> ! {
        self.print();
        std::process::exit(if self.is_display() { 0 } else { 2 })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.render().trim_end())
    }
}

impl std::error::Error for Error {}

impl Error {
    /// A subcommand was needed and none was matched. (Used by the derive.)
    #[must_use]
    pub fn missing_subcommand() -> Self {
        Self::new(ErrorKind::MissingSubcommand, "a subcommand is required")
    }
}
