//! The one error type for every codec in this crate.

use std::fmt;

/// A failure to parse, serialise or convert data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    /// 1-based line and column in the input, when the error has a place.
    location: Option<(usize, usize)>,
    /// Where in the data the problem is: `user.tags[2]`.
    path: String,
}

impl Error {
    pub fn new(message: impl fmt::Display) -> Self {
        Self {
            message: message.to_string(),
            location: None,
            path: String::new(),
        }
    }

    /// An error at `offset` bytes into `text`.
    pub fn at(message: impl fmt::Display, text: &str, offset: usize) -> Self {
        let upto = &text.as_bytes()[..offset.min(text.len())];
        #[allow(clippy::naive_bytecount)]
        let line = upto.iter().filter(|&&b| b == b'\n').count() + 1;
        let column = upto.iter().rev().take_while(|&&b| b != b'\n').count() + 1;
        Self {
            message: message.to_string(),
            location: Some((line, column)),
            path: String::new(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// `(line, column)`, both 1-based.
    #[must_use]
    pub const fn location(&self) -> Option<(usize, usize)> {
        self.location
    }

    /// The location in the data (`user.tags[2]`), empty at the top level.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Notes that the problem is inside the field `name` of the current value.
    #[must_use]
    pub fn in_field(mut self, name: &str) -> Self {
        self.path = if self.path.is_empty() || self.path.starts_with('[') {
            format!("{name}{}", self.path)
        } else {
            format!("{name}.{}", self.path)
        };
        self
    }

    /// Notes that the problem is inside element `index` of the current value.
    #[must_use]
    pub fn in_index(mut self, index: usize) -> Self {
        self.path = if self.path.is_empty() || self.path.starts_with('[') {
            format!("[{index}]{}", self.path)
        } else {
            format!("[{index}].{}", self.path)
        };
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.is_empty() {
            write!(f, "{}: ", self.path)?;
        }
        match self.location {
            Some((line, column)) => write!(f, "{} at line {line} column {column}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_errors_display_their_message() {
        let e = Error::new("boom");
        assert_eq!(e.to_string(), "boom");
        assert_eq!(e.message(), "boom");
        assert_eq!(e.location(), None);
    }

    #[test]
    fn locations_count_lines_and_columns() {
        let text = "ab\ncd\nef";
        assert_eq!(Error::at("x", text, 0).location(), Some((1, 1)));
        assert_eq!(Error::at("x", text, 1).location(), Some((1, 2)));
        assert_eq!(Error::at("x", text, 3).location(), Some((2, 1)));
        assert_eq!(Error::at("x", text, 7).location(), Some((3, 2)));
        // past the end clamps
        assert_eq!(Error::at("x", text, 99).location(), Some((3, 3)));
        assert_eq!(
            Error::at("bad", text, 4).to_string(),
            "bad at line 2 column 2"
        );
    }

    #[test]
    fn paths_build_from_the_inside_out() {
        let e = Error::new("expected a string")
            .in_index(2)
            .in_field("tags")
            .in_field("user");
        assert_eq!(e.path(), "user.tags[2]");
        assert_eq!(e.to_string(), "user.tags[2]: expected a string");
        let nested = Error::new("bad").in_index(1).in_index(0).in_field("grid");
        assert_eq!(nested.path(), "grid[0][1]");
        let deep = Error::new("bad")
            .in_field("c")
            .in_index(3)
            .in_field("b")
            .in_field("a");
        assert_eq!(deep.path(), "a.b[3].c");
        assert_eq!(Error::new("x").path(), "");
    }

    #[test]
    fn path_and_location_display_together() {
        let e = Error::at("oops", "a\nb", 2).in_field("f");
        assert_eq!(e.to_string(), "f: oops at line 2 column 1");
    }
}
