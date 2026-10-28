//! Component values: the tokens of a declaration's value, with functions and blocks nested.

use crate::token::Token;

/// One component of a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Ident(String),
    /// A bare number.
    Number(f32),
    Percent(f32),
    /// A number with a unit: `16px` is `Dimension(16.0, "px")`.
    Dimension(f32, String),
    Hash(String),
    Str(String),
    /// `name(args)`.
    Function(String, Vec<Value>),
    /// A parenthesised, bracketed or braced group.
    Block(char, Vec<Value>),
    Comma,
    Delim(char),
}

/// Builds the component values of `tokens` (whitespace separates them and is dropped; a group or function is
/// cut at its closing bracket).
#[must_use]
pub fn parse_values(tokens: &[Token]) -> Vec<Value> {
    let mut at = 0;
    values_until(tokens, &mut at, None)
}

fn values_until(tokens: &[Token], at: &mut usize, close: Option<char>) -> Vec<Value> {
    let mut out = Vec::new();
    while *at < tokens.len() {
        let token = &tokens[*at];
        *at += 1;
        match token {
            Token::Whitespace => {}
            Token::Close(c) if Some(*c) == close => return out,
            Token::Close(_) | Token::Semicolon | Token::Colon => {
                out.push(Value::Delim(match token {
                    Token::Close(c) => *c,
                    Token::Semicolon => ';',
                    _ => ':',
                }));
            }
            Token::Ident(s) => out.push(Value::Ident(s.clone())),
            Token::AtKeyword(s) => out.push(Value::Ident(format!("@{s}"))),
            Token::Hash(s) => out.push(Value::Hash(s.clone())),
            Token::Str(s) => out.push(Value::Str(s.clone())),
            Token::Number { value, unit } => out.push(match unit.as_str() {
                "" => Value::Number(*value),
                "%" => Value::Percent(*value),
                _ => Value::Dimension(*value, unit.clone()),
            }),
            Token::Delim(c) => out.push(Value::Delim(*c)),
            Token::Comma => out.push(Value::Comma),
            Token::Function(name) => {
                let args = values_until(tokens, at, Some(')'));
                out.push(Value::Function(name.to_ascii_lowercase(), args));
            }
            Token::Open(c) => {
                let closing = match c {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                let inner = values_until(tokens, at, Some(closing));
                out.push(Value::Block(*c, inner));
            }
        }
    }
    out
}

impl Value {
    /// The identifier, lowercased, if this is one.
    #[must_use]
    pub fn ident(&self) -> Option<String> {
        match self {
            Self::Ident(s) => Some(s.to_ascii_lowercase()),
            _ => None,
        }
    }

    /// The numeric value of a bare number (`0` and `1.5`); a dimension is not one.
    #[must_use]
    pub const fn number(&self) -> Option<f32> {
        match self {
            Self::Number(n) => Some(*n),
            _ => None,
        }
    }
}

/// Splits values at top-level commas: `a b, c` is `[[a, b], [c]]`.
#[must_use]
pub fn split_commas(values: &[Value]) -> Vec<Vec<Value>> {
    let mut parts = vec![Vec::new()];
    for v in values {
        if *v == Value::Comma {
            parts.push(Vec::new());
        } else {
            parts.last_mut().expect("at least one part").push(v.clone());
        }
    }
    parts
}
