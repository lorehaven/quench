//! Coloured and emphasised text (the part of `crossterm::style` the tools use).
//!
//! `"text".bold().cyan()` is a value that displays as the text wrapped in ANSI codes. Setting the `NO_COLOR`
//! environment variable to anything non-empty turns the codes off.

use std::fmt;

/// The sixteen ANSI colours.
///
/// The plain names are the bright ones (`Red` is `\x1b[91m`), the `Dark` names the normal ones (`DarkRed` is
/// `\x1b[31m`), and `Grey` is the light grey that sits between `DarkGrey` and `White`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Black,
    DarkGrey,
    Grey,
    White,
    Red,
    DarkRed,
    Green,
    DarkGreen,
    Yellow,
    DarkYellow,
    Blue,
    DarkBlue,
    Magenta,
    DarkMagenta,
    Cyan,
    DarkCyan,
}

impl Color {
    const fn code(self) -> u8 {
        match self {
            Self::Black => 30,
            Self::DarkRed => 31,
            Self::DarkGreen => 32,
            Self::DarkYellow => 33,
            Self::DarkBlue => 34,
            Self::DarkMagenta => 35,
            Self::DarkCyan => 36,
            Self::Grey => 37,
            Self::DarkGrey => 90,
            Self::Red => 91,
            Self::Green => 92,
            Self::Yellow => 93,
            Self::Blue => 94,
            Self::Magenta => 95,
            Self::Cyan => 96,
            Self::White => 97,
        }
    }
}

/// A value together with how to show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Styled<T> {
    value: T,
    color: Option<Color>,
    bold: bool,
    dim: bool,
    underlined: bool,
}

impl<T> Styled<T> {
    const fn new(value: T) -> Self {
        Self {
            value,
            color: None,
            bold: false,
            dim: false,
            underlined: false,
        }
    }

    /// The value without its styling.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.value
    }

    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    #[must_use]
    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    #[must_use]
    pub const fn underlined(mut self) -> Self {
        self.underlined = true;
        self
    }

    #[must_use]
    pub const fn with(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    #[must_use]
    pub const fn black(self) -> Self {
        self.with(Color::Black)
    }

    #[must_use]
    pub const fn dark_grey(self) -> Self {
        self.with(Color::DarkGrey)
    }

    #[must_use]
    pub const fn grey(self) -> Self {
        self.with(Color::Grey)
    }

    #[must_use]
    pub const fn white(self) -> Self {
        self.with(Color::White)
    }

    #[must_use]
    pub const fn red(self) -> Self {
        self.with(Color::Red)
    }

    #[must_use]
    pub const fn dark_red(self) -> Self {
        self.with(Color::DarkRed)
    }

    #[must_use]
    pub const fn green(self) -> Self {
        self.with(Color::Green)
    }

    #[must_use]
    pub const fn dark_green(self) -> Self {
        self.with(Color::DarkGreen)
    }

    #[must_use]
    pub const fn yellow(self) -> Self {
        self.with(Color::Yellow)
    }

    #[must_use]
    pub const fn dark_yellow(self) -> Self {
        self.with(Color::DarkYellow)
    }

    #[must_use]
    pub const fn blue(self) -> Self {
        self.with(Color::Blue)
    }

    #[must_use]
    pub const fn dark_blue(self) -> Self {
        self.with(Color::DarkBlue)
    }

    #[must_use]
    pub const fn magenta(self) -> Self {
        self.with(Color::Magenta)
    }

    #[must_use]
    pub const fn dark_magenta(self) -> Self {
        self.with(Color::DarkMagenta)
    }

    #[must_use]
    pub const fn cyan(self) -> Self {
        self.with(Color::Cyan)
    }

    #[must_use]
    pub const fn dark_cyan(self) -> Self {
        self.with(Color::DarkCyan)
    }
}

impl<T: fmt::Display> fmt::Display for Styled<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plain = !(self.bold || self.dim || self.underlined || self.color.is_some())
            || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        if plain {
            return self.value.fmt(f);
        }
        let mut codes = Vec::new();
        if self.bold {
            codes.push(1);
        }
        if self.dim {
            codes.push(2);
        }
        if self.underlined {
            codes.push(4);
        }
        if let Some(color) = self.color {
            codes.push(color.code());
        }
        let codes = codes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(";");
        write!(f, "\x1b[{codes}m{}\x1b[0m", self.value)
    }
}

/// Starts styling any displayable value; further calls on the result adjust the same [`Styled`].
pub trait Stylize: fmt::Display + Sized {
    fn styled(self) -> Styled<Self> {
        Styled::new(self)
    }

    fn bold(self) -> Styled<Self> {
        self.styled().bold()
    }

    fn dim(self) -> Styled<Self> {
        self.styled().dim()
    }

    fn underlined(self) -> Styled<Self> {
        self.styled().underlined()
    }

    fn black(self) -> Styled<Self> {
        self.styled().black()
    }

    fn dark_grey(self) -> Styled<Self> {
        self.styled().dark_grey()
    }

    fn grey(self) -> Styled<Self> {
        self.styled().grey()
    }

    fn white(self) -> Styled<Self> {
        self.styled().white()
    }

    fn red(self) -> Styled<Self> {
        self.styled().red()
    }

    fn dark_red(self) -> Styled<Self> {
        self.styled().dark_red()
    }

    fn green(self) -> Styled<Self> {
        self.styled().green()
    }

    fn dark_green(self) -> Styled<Self> {
        self.styled().dark_green()
    }

    fn yellow(self) -> Styled<Self> {
        self.styled().yellow()
    }

    fn dark_yellow(self) -> Styled<Self> {
        self.styled().dark_yellow()
    }

    fn blue(self) -> Styled<Self> {
        self.styled().blue()
    }

    fn dark_blue(self) -> Styled<Self> {
        self.styled().dark_blue()
    }

    fn magenta(self) -> Styled<Self> {
        self.styled().magenta()
    }

    fn dark_magenta(self) -> Styled<Self> {
        self.styled().dark_magenta()
    }

    fn cyan(self) -> Styled<Self> {
        self.styled().cyan()
    }

    fn dark_cyan(self) -> Styled<Self> {
        self.styled().dark_cyan()
    }
}

impl<T: fmt::Display> Stylize for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // `NO_COLOR` is process-wide, so every assertion about it lives in one test.
    #[test]
    fn codes_wrap_the_text_unless_no_color_is_set() {
        // SAFETY of env use: this is the only test in the crate that touches `NO_COLOR`.
        let saved = std::env::var_os("NO_COLOR");
        quench_core::env::remove("NO_COLOR");

        assert_eq!("hi".to_string(), format!("{}", "hi".styled()));
        assert_eq!(format!("{}", "hi".bold()), "\x1b[1mhi\x1b[0m");
        assert_eq!(format!("{}", "hi".cyan()), "\x1b[96mhi\x1b[0m");
        assert_eq!(format!("{}", "hi".dark_cyan()), "\x1b[36mhi\x1b[0m");
        assert_eq!(
            format!("{}", "hi".bold().underlined().white()),
            "\x1b[1;4;97mhi\x1b[0m"
        );
        assert_eq!(format!("{}", 42.green().dim()), "\x1b[2;92m42\x1b[0m");
        assert_eq!("x".grey().into_inner(), "x");
        for (styled, code) in [
            ("a".black().to_string(), 30),
            ("a".dark_red().to_string(), 31),
            ("a".dark_green().to_string(), 32),
            ("a".dark_yellow().to_string(), 33),
            ("a".dark_blue().to_string(), 34),
            ("a".dark_magenta().to_string(), 35),
            ("a".dark_cyan().to_string(), 36),
            ("a".grey().to_string(), 37),
            ("a".dark_grey().to_string(), 90),
            ("a".red().to_string(), 91),
            ("a".green().to_string(), 92),
            ("a".yellow().to_string(), 93),
            ("a".blue().to_string(), 94),
            ("a".magenta().to_string(), 95),
            ("a".cyan().to_string(), 96),
            ("a".white().to_string(), 97),
        ] {
            assert_eq!(styled, format!("\x1b[{code}ma\x1b[0m"));
        }

        quench_core::env::set("NO_COLOR", "1");
        assert_eq!(format!("{}", "hi".bold().red()), "hi");
        quench_core::env::set("NO_COLOR", "");
        assert_eq!(format!("{}", "hi".bold()), "\x1b[1mhi\x1b[0m");

        match saved {
            Some(value) => quench_core::env::set("NO_COLOR", value),
            None => quench_core::env::remove("NO_COLOR"),
        }
    }
}
