//! TeX math with no dependencies: a reader for the math people write in Markdown pages (`$…$`, `$$…$$`), and
//! its text output, one line of Unicode for inline math or a two-dimensional block of terminal cells for
//! display math.
//!
//! [`parse`] reads a formula into a [`Formula`], a syntax tree that keeps what TeX's layout needs (atom
//! classes, limits, styles, delimiters, arrays), so a typesetter can work from the same tree. A formula with
//! anything the reader does not know is an [`Error`], never a partial result, so a caller can show the TeX
//! source instead of half a translation. [`parse_with`] keeps `\newcommand` and `\def` definitions from one
//! formula to the next, as a page's formulas expect.
//!
//! With the `typeset` feature, [`typeset`](mod@typeset) lays a formula out by TeX's rules with an OpenType
//! MATH font (Noto Sans Math, embedded) and draws it to PNG or SVG.
//!
//! ```
//! use quench_tex::{parse, to_text_block, to_unicode};
//!
//! let formula = parse(r"\sum_{i=1}^n i^2 \le \frac{n^3}{2} \quad \forall n \in \mathbb{N}")?;
//! assert_eq!(to_unicode(&formula), "∑ᵢ₌₁ⁿ i² ≤ n³/2  ∀n ∈ ℕ");
//!
//! let block = to_text_block(&parse(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}")?);
//! assert_eq!(block.lines, [
//!     "          ________",
//!     "    −b ± √b² − 4ac",
//!     "x = ──────────────",
//!     "          2a",
//! ]);
//! # Ok::<(), quench_tex::Error>(())
//! ```

mod ast;
mod lexer;
mod macros;
mod parse;
mod symbols;
mod text;
#[cfg(feature = "typeset")]
pub mod typeset;
mod unicode;

pub use ast::{
    Accent, Align, Array, ArrayKind, Class, Column, Formula, Limits, MathChar, MathFont, Node,
    OpName, Space, Style, TextFont, TextPart,
};
pub use macros::Macros;
pub use symbols::{large_op, named_op, symbol};
pub use text::{TextBlock, to_text_block, to_unicode};
pub use unicode::{display_width, math_alphanumeric};

use std::fmt;

/// Why a formula could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A command or environment the reader does not know (or a form of one it does not support).
    Unsupported {
        /// The command as written, with its backslash (`\foo`, `\begin{tabular}`).
        command: String,
        /// The byte offset in the source where it is.
        at: usize,
    },
    /// The formula is not well-formed TeX.
    Syntax {
        /// The byte offset in the source where the problem was found.
        at: usize,
        /// What is wrong.
        message: String,
    },
}

impl Error {
    pub(crate) fn syntax(at: usize, message: impl Into<String>) -> Self {
        Self::Syntax {
            at,
            message: message.into(),
        }
    }

    /// The byte offset in the source the error is at.
    #[must_use]
    pub const fn at(&self) -> usize {
        match self {
            Self::Unsupported { at, .. } | Self::Syntax { at, .. } => *at,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { command, at } => {
                write!(f, "unsupported TeX command `{command}` at byte {at}")
            }
            Self::Syntax { at, message } => write!(f, "TeX syntax error at byte {at}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

/// Reads a formula: the TeX between the `$` signs, without them.
///
/// # Errors
/// [`Error::Unsupported`] for a command or environment the reader does not know, [`Error::Syntax`] for TeX that
/// is not well-formed (unbalanced braces, a missing argument, a double script, nesting deeper than 150, a
/// runaway macro, ...). The whole formula fails: there is no partial result.
pub fn parse(tex: &str) -> Result<Formula, Error> {
    parse::parse(tex, &mut Macros::new())
}

/// Reads a formula with the macros defined so far.
///
/// Keeps the definitions it makes (`\newcommand`, `\def`, `\DeclareMathOperator`) in `macros` for the formulas
/// after it. A formula that fails changes nothing.
///
/// # Errors
/// As [`parse`]; `macros` is then left as it was.
pub fn parse_with(tex: &str, macros: &mut Macros) -> Result<Formula, Error> {
    let mut scratch = macros.clone();
    let formula = parse::parse(tex, &mut scratch)?;
    *macros = scratch;
    Ok(formula)
}
