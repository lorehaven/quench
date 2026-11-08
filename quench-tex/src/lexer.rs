//! TeX's tokens from the source: control sequences, characters with their category, spaces and parameters.
//!
//! The categories are TeX's plain ones and never change (no `\catcode`): `{` `}` group, `^` `_` scripts, `&`
//! alignment, `#` parameter, `$` math shift, `~` a tie, `%` a comment to the end of the line. Runs of white
//! space are one space token, and spaces after a control word are dropped, as TeX's reader does.

use crate::Error;

/// What a token is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// A character of no special category.
    Char(char),
    /// A control sequence, without the backslash: `alpha`, or one non-letter like `,` or `\`.
    Command(String),
    /// `{`.
    Begin,
    /// `}`.
    End,
    /// `^`.
    Sup,
    /// `_`.
    Sub,
    /// `&`.
    Align,
    /// `#1` ... `#9`.
    Param(u8),
    /// `$`.
    Shift,
    /// `~`.
    Tie,
    /// White space.
    Space,
}

/// A token and the byte offset in the source it was read at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The token.
    pub tok: Tok,
    /// The byte offset of its first character.
    pub at: usize,
}

/// Splits `source` into tokens.
pub fn tokenize(source: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let tok = match c {
            '\\' => {
                let Some(&(_, first)) = chars.peek() else {
                    return Err(Error::syntax(at, "a lone `\\` at the end"));
                };
                chars.next();
                if first.is_ascii_alphabetic() {
                    let mut name = String::from(first);
                    while let Some(&(_, c)) = chars.peek() {
                        if !c.is_ascii_alphabetic() {
                            break;
                        }
                        name.push(c);
                        chars.next();
                    }
                    while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
                        chars.next();
                    }
                    Tok::Command(name)
                } else {
                    Tok::Command(first.to_string())
                }
            }
            '%' => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            '#' => match chars.peek() {
                Some(&(_, d @ '1'..='9')) => {
                    chars.next();
                    Tok::Param(d as u8 - b'0')
                }
                // A colour (`\color{#ff0000}`); anywhere else the reader rejects it.
                _ => Tok::Char('#'),
            },
            c if c.is_whitespace() => {
                while chars.peek().is_some_and(|&(_, c)| c.is_whitespace()) {
                    chars.next();
                }
                if tokens.last().is_some_and(|t: &Token| t.tok == Tok::Space) {
                    continue;
                }
                Tok::Space
            }
            '{' => Tok::Begin,
            '}' => Tok::End,
            '^' => Tok::Sup,
            '_' => Tok::Sub,
            '&' => Tok::Align,
            '$' => Tok::Shift,
            '~' => Tok::Tie,
            c => Tok::Char(c),
        };
        tokens.push(Token { tok, at });
    }
    Ok(tokens)
}
