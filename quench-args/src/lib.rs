//! Command-line argument parsing with derive macros, a drop-in for the part of `clap` the tools use.
//!
//! ```
//! use quench_args::{Parser, Subcommand, ValueEnum};
//!
//! #[derive(Parser, Debug)]
//! #[command(name = "tool", version)]
//! struct Cli {
//!     /// Environment to act on.
//!     #[arg(long, short = 'e', global = true, value_name = "ENV")]
//!     env: Option<String>,
//!     #[command(subcommand)]
//!     cmd: Cmd,
//! }
//!
//! #[derive(Subcommand, Debug)]
//! enum Cmd {
//!     /// Render the manifests
//!     Render {
//!         #[arg(long, value_enum, default_value_t = Scope::All)]
//!         scope: Scope,
//!         targets: Vec<String>,
//!     },
//!     /// Say hello
//!     #[command(visible_alias = "hi")]
//!     Hello,
//! }
//!
//! #[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
//! enum Scope { All, Mutable }
//!
//! let cli = Cli::try_parse_from(["tool", "render", "--scope", "mutable", "a/b"]).unwrap();
//! assert!(matches!(cli.cmd, Cmd::Render { scope: Scope::Mutable, .. }));
//! ```
//!
//! The derives and their attributes (`#[command(..)]`, `#[arg(..)]`, `#[value(..)]`) follow clap's names, so a
//! tool switches by changing its `use`. What is supported is listed in the crate docs; anything else is a
//! compile error that says so.

extern crate self as quench_args;

mod command;
mod error;
mod help;
mod matches;
mod parse;

pub use command::{Arg, Command, Kind, PossibleValue};
pub use error::{Error, ErrorKind};
pub use matches::Matches;
pub use quench_args_macros::{Args, Parser, Subcommand, ValueEnum};

use std::ffi::OsString;
use std::fmt::Display;
use std::str::FromStr;

impl Command {
    /// Parses `args` (without the program name).
    ///
    /// # Errors
    ///
    /// The first mistake found, or [`ErrorKind::DisplayHelp`] / [`ErrorKind::DisplayVersion`] when the user
    /// asked for those.
    pub fn try_get_matches_from<I, T>(&self, args: I) -> Result<Matches, Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        let mut words = Vec::new();
        for arg in args {
            let arg: OsString = arg.into();
            words.push(arg.into_string().map_err(|bad| {
                Error::new(
                    ErrorKind::InvalidUtf8,
                    format!(
                        "invalid UTF-8 was detected in one or more arguments: {}",
                        bad.to_string_lossy()
                    ),
                )
            })?);
        }
        parse::parse(self, &words)
    }

    /// The text `-h` prints.
    #[must_use]
    pub fn render_help(&self) -> String {
        help::render_help(self, std::slice::from_ref(&self.name), &[], false)
    }

    /// The text `--help` prints.
    #[must_use]
    pub fn render_long_help(&self) -> String {
        help::render_help(self, std::slice::from_ref(&self.name), &[], true)
    }

    /// Prints [`render_help`](Self::render_help) to standard output.
    ///
    /// # Errors
    ///
    /// When standard output cannot be written.
    pub fn print_help(&self) -> std::io::Result<()> {
        use std::io::Write as _;
        let mut out = std::io::stdout().lock();
        out.write_all(self.render_help().as_bytes())?;
        out.flush()
    }

    /// The help of the subcommand reached through `path` (names or aliases, outermost first), with the usage
    /// line showing the whole path (`Usage: tool env set ..`), or `None` when some name is not a subcommand.
    #[must_use]
    pub fn help_for(&self, path: &[&str], long: bool) -> Option<String> {
        let mut cmd = self;
        let mut names = vec![self.name.clone()];
        let mut globals: Vec<&Arg> = Vec::new();
        for word in path {
            globals.extend(cmd.args.iter().filter(|a| a.global));
            cmd = cmd.find_subcommand(word)?;
            names.push(cmd.name.clone());
        }
        Some(help::render_help(cmd, &names, &globals, long))
    }
}

/// Reads a typed value from the text of one argument.
pub trait FromArg: Sized {
    /// # Errors
    ///
    /// [`ErrorKind::InvalidValue`] naming the argument and the text.
    fn from_arg(arg: &str, raw: &str) -> Result<Self, Error>;
}

impl<T: FromStr> FromArg for T
where
    T::Err: Display,
{
    fn from_arg(arg: &str, raw: &str) -> Result<Self, Error> {
        raw.parse()
            .map_err(|e: T::Err| Error::invalid_value(arg, raw, &e.to_string()))
    }
}

/// An enum whose variants are the accepted spellings of a value.
pub trait ValueEnum: Sized + Clone + 'static {
    fn value_variants() -> &'static [Self];

    /// The spelling of this variant, or `None` to hide it.
    fn to_possible_value(&self) -> Option<PossibleValue>;

    /// The variant spelled `input`.
    ///
    /// # Errors
    ///
    /// A message when nothing matches.
    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> {
        Self::value_variants()
            .iter()
            .find(|variant| {
                variant.to_possible_value().is_some_and(|p| {
                    if ignore_case {
                        p.get_name().eq_ignore_ascii_case(input)
                    } else {
                        p.get_name() == input
                    }
                })
            })
            .cloned()
            .ok_or_else(|| format!("invalid variant: {input}"))
    }
}

/// Reads a [`ValueEnum`] from an argument's text. (Used by the derive.)
///
/// # Errors
///
/// [`ErrorKind::InvalidValue`].
pub fn value_enum_arg<T: ValueEnum>(arg: &str, raw: &str) -> Result<T, Error> {
    T::from_str(raw, false).map_err(|reason| Error::invalid_value(arg, raw, &reason))
}

/// The possible values of a [`ValueEnum`], for an [`Arg`]. (Used by the derive.)
#[must_use]
pub fn possible_values<T: ValueEnum>() -> Vec<PossibleValue> {
    T::value_variants()
        .iter()
        .filter_map(ValueEnum::to_possible_value)
        .collect()
}

/// Builds a type from what was matched.
pub trait FromMatches: Sized {
    /// # Errors
    ///
    /// When a value does not parse.
    fn from_matches(matches: &Matches) -> Result<Self, Error>;
}

/// A type that describes a whole command line.
pub trait CommandFactory {
    fn command() -> Command;
}

/// A group of arguments that can be added to a command.
pub trait Args: FromMatches {
    fn augment_args(cmd: Command) -> Command;
}

/// An enum of subcommands.
pub trait Subcommand: FromMatches {
    fn augment_subcommands(cmd: Command) -> Command;
}

/// A type read from the command line: `Cli::parse()`.
pub trait Parser: CommandFactory + FromMatches {
    /// Reads the process arguments; prints help or the error and exits when it must.
    #[must_use]
    fn parse() -> Self {
        Self::parse_from(std::env::args_os())
    }

    /// Like [`parse`](Self::parse), returning the error instead of exiting.
    ///
    /// # Errors
    ///
    /// As [`Command::try_get_matches_from`].
    fn try_parse() -> Result<Self, Error> {
        Self::try_parse_from(std::env::args_os())
    }

    /// Reads `args` (the first is the program name); exits on help, version or a mistake.
    #[must_use]
    fn parse_from<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        Self::try_parse_from(args).unwrap_or_else(|e| e.exit())
    }

    /// Reads `args` (the first is the program name).
    ///
    /// # Errors
    ///
    /// As [`Command::try_get_matches_from`].
    fn try_parse_from<I, T>(args: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let command = Self::command();
        let matches = command.try_get_matches_from(args.into_iter().skip(1))?;
        Self::from_matches(&matches)
    }
}

#[cfg(test)]
mod tests;
