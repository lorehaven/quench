//! SQL text to tokens.

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A bare word (keyword or name) or a quoted name.
    Word {
        text: String,
        quoted: bool,
    },
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
    /// `?`, `?12`, `:name`, `@name`, `$name`, as written.
    Param(String),
    Op(&'static str),
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
}

fn syntax(pos: usize, message: impl Into<String>) -> Error {
    Error::Syntax {
        pos,
        message: message.into(),
    }
}

const OPERATORS: [&str; 26] = [
    "->>", "->", "||", "<=", ">=", "<>", "!=", "==", "<<", ">>", "(", ")", ",", ";", ".", "*", "+",
    "-", "/", "%", "=", "<", ">", "&", "|", "~",
];

const fn is_word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

const fn is_word_char(b: u8) -> bool {
    is_word_start(b) || b.is_ascii_digit() || b == b'$'
}

/// Splits `src` into tokens, ending with `Eof`.
pub fn tokenize(src: &str) -> Result<Vec<Token>> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let start = i;
        if b.is_ascii_whitespace() {
            i += 1;
        } else if b == b'-' && bytes.get(i + 1) == Some(&b'-') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let end = src[i + 2..]
                .find("*/")
                .ok_or_else(|| syntax(i, "unterminated comment"))?;
            i += end + 4;
        } else if (b == b'x' || b == b'X') && bytes.get(i + 1) == Some(&b'\'') {
            let end = src[i + 2..]
                .find('\'')
                .ok_or_else(|| syntax(i, "unterminated blob literal"))?;
            out.push(Token {
                kind: Kind::Blob(
                    hex(&src[i + 2..i + 2 + end])
                        .ok_or_else(|| syntax(i, "malformed blob literal"))?,
                ),
                start,
            });
            i += end + 3;
        } else if is_word_start(b) {
            while i < bytes.len() && is_word_char(bytes[i]) {
                i += 1;
            }
            out.push(Token {
                kind: Kind::Word {
                    text: src[start..i].to_string(),
                    quoted: false,
                },
                start,
            });
        } else if b.is_ascii_digit()
            || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let (kind, end) = number(src, i)?;
            out.push(Token { kind, start });
            i = end;
        } else if b == b'\'' {
            let (text, end) = quoted(src, i, b'\'')?;
            out.push(Token {
                kind: Kind::Text(text),
                start,
            });
            i = end;
        } else if b == b'"' || b == b'`' {
            let (text, end) = quoted(src, i, b)?;
            out.push(Token {
                kind: Kind::Word { text, quoted: true },
                start,
            });
            i = end;
        } else if b == b'[' {
            let end = src[i + 1..]
                .find(']')
                .ok_or_else(|| syntax(i, "unterminated [name]"))?;
            out.push(Token {
                kind: Kind::Word {
                    text: src[i + 1..i + 1 + end].to_string(),
                    quoted: true,
                },
                start,
            });
            i += end + 2;
        } else if b == b'?' || b == b':' || b == b'@' || b == b'$' {
            i += 1;
            while i < bytes.len()
                && (is_word_char(bytes[i]) || (b == b'?' && bytes[i].is_ascii_digit()))
            {
                i += 1;
            }
            if b != b'?' && i == start + 1 {
                return Err(syntax(start, "a parameter name is missing"));
            }
            out.push(Token {
                kind: Kind::Param(src[start..i].to_string()),
                start,
            });
        } else if let Some(op) = OPERATORS.iter().find(|op| src[i..].starts_with(**op)) {
            out.push(Token {
                kind: Kind::Op(op),
                start,
            });
            i += op.len();
        } else {
            return Err(syntax(
                i,
                format!(
                    "unexpected character {:?}",
                    src[i..].chars().next().unwrap_or('?')
                ),
            ));
        }
    }
    out.push(Token {
        kind: Kind::Eof,
        start: src.len(),
    });
    Ok(out)
}

fn hex(digits: &str) -> Option<Vec<u8>> {
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(digits.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A quoted run starting at `start` (the quote), the quote doubled to put one in: the text and where it ends.
fn quoted(src: &str, start: usize, quote: u8) -> Result<(String, usize)> {
    let bytes = src.as_bytes();
    let mut text = String::new();
    let mut i = start + 1;
    let mut run = i;
    while i < bytes.len() {
        if bytes[i] == quote {
            text.push_str(&src[run..i]);
            if bytes.get(i + 1) == Some(&quote) {
                text.push(quote as char);
                i += 2;
                run = i;
            } else {
                return Ok((text, i + 1));
            }
        } else {
            i += 1;
        }
    }
    Err(syntax(start, "unterminated quoted text"))
}

fn number(src: &str, start: usize) -> Result<(Kind, usize)> {
    let bytes = src.as_bytes();
    if bytes[start] == b'0' && matches!(bytes.get(start + 1), Some(b'x' | b'X')) {
        let mut i = start + 2;
        while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
            i += 1;
        }
        let digits = &src[start + 2..i];
        let value = u64::from_str_radix(digits, 16)
            .map_err(|_| syntax(start, "malformed hexadecimal integer"))?;
        // Hexadecimal literals are the two's complement bit pattern.
        return Ok((Kind::Integer(value as i64), i));
    }
    let mut i = start;
    let mut real = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        real = true;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(bytes.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if bytes.get(j).is_some_and(u8::is_ascii_digit) {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            real = true;
            i = j;
        }
    }
    if bytes.get(i).is_some_and(|b| is_word_start(*b)) {
        return Err(syntax(start, "a number runs into a word"));
    }
    let text = &src[start..i];
    if !real && let Ok(n) = text.parse::<i64>() {
        return Ok((Kind::Integer(n), i));
    }
    // An integer too large for 64 bits is a real, as in SQLite.
    let n: f64 = text
        .parse()
        .map_err(|_| syntax(start, "malformed number"))?;
    Ok((Kind::Float(n), i))
}
