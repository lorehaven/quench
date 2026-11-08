//! User macros: what `\newcommand`, `\renewcommand`, `\providecommand`, `\DeclareMathOperator` and `\def`
//! define, kept across the formulas of a page.
//!
//! A macro is a list of tokens with `#1`…`#9` in it. Expansion replaces the call and its arguments by the body
//! with the arguments substituted, then reading goes on from the start of the result, as TeX's does. The number
//! of expansions in one formula is bounded, so a recursive definition is an error and not a hang.

use crate::Error;
use crate::lexer::{Tok, Token, tokenize};
use std::collections::HashMap;

/// The most expansions one formula may do.
pub const MAX_EXPANSIONS: usize = 10_000;

/// The most tokens waiting to be read that expansion may produce (an exponentially growing definition).
pub const MAX_PENDING: usize = 100_000;

/// A macro definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macro {
    /// How many arguments it takes.
    pub params: u8,
    /// The default of an optional first argument (`\newcommand{\a}[2][x]{…}`), if it has one.
    pub default: Option<Vec<Token>>,
    /// The body, with `Param` tokens for the arguments.
    pub body: Vec<Token>,
}

/// A set of macro definitions, carried from formula to formula with [`crate::parse_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Macros {
    defs: HashMap<String, Macro>,
}

impl Macros {
    /// No definitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Defines `\name` (given without the backslash) as `body`, which uses `#1`…`#params` for its arguments, as
    /// `\newcommand{\name}[params]{body}` does. Redefines an existing macro.
    ///
    /// # Errors
    /// [`Error::Syntax`] (at byte 0, or at the offending token of `body`) when `params` is more than 9, when
    /// `body` cannot be tokenized (a lone `\` at its end), or when it uses a parameter above `params`. Nothing
    /// is defined then.
    pub fn define(&mut self, name: &str, params: u8, body: &str) -> Result<(), Error> {
        if params > 9 {
            return Err(Error::syntax(
                0,
                format!("`\\{name}` takes 0 to 9 arguments, not `{params}`"),
            ));
        }
        let body = tokenize(body)?;
        check_params(&body, params, 0)?;
        self.insert(
            name.to_owned(),
            Macro {
                params,
                default: None,
                body,
            },
        );
        Ok(())
    }

    /// Whether `\name` (without the backslash) is defined.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.defs.contains_key(name)
    }

    /// How many macros are defined.
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Macro> {
        self.defs.get(name)
    }

    pub(crate) fn insert(&mut self, name: String, definition: Macro) {
        self.defs.insert(name, definition);
    }
}

/// Checks that the body uses no parameter beyond `params`.
pub fn check_params(body: &[Token], params: u8, at: usize) -> Result<(), Error> {
    body.iter()
        .find(|t| matches!(t.tok, Tok::Param(n) if n > params))
        .map_or(Ok(()), |t| {
            Err(Error::syntax(
                at.max(t.at),
                format!(
                    "the body uses #{} but the macro takes {params} arguments",
                    param_number(&t.tok)
                ),
            ))
        })
}

const fn param_number(tok: &Tok) -> u8 {
    match tok {
        Tok::Param(n) => *n,
        _ => 0,
    }
}

/// The body of `definition` with `args` substituted; the body's tokens are placed at `at` (the call's position),
/// the arguments keep theirs.
pub fn substitute(definition: &Macro, args: &[Vec<Token>], at: usize) -> Vec<Token> {
    let mut out = Vec::with_capacity(definition.body.len());
    for token in &definition.body {
        match token.tok {
            Tok::Param(n) => {
                if let Some(arg) = args.get(usize::from(n) - 1) {
                    out.extend(arg.iter().cloned());
                }
            }
            _ => out.push(Token {
                tok: token.tok.clone(),
                at,
            }),
        }
    }
    out
}
