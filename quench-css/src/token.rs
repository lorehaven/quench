//! Tokens: the CSS syntax cut into pieces (a subset of CSS Syntax Level 3 that covers real stylesheets).

/// One piece of a stylesheet.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    /// A function's name and its opening parenthesis: `calc(`.
    Function(String),
    /// `@media`, without the `@`.
    AtKeyword(String),
    /// `#fff`, without the `#`.
    Hash(String),
    Str(String),
    /// A number, with the unit after it (`16px`, `1.5rem`, `50%` as unit `%`, a bare `2` as unit "").
    Number {
        value: f32,
        unit: String,
    },
    Delim(char),
    Whitespace,
    Colon,
    Semicolon,
    Comma,
    /// `{`, `}`, `(`, `)`, `[`, `]`.
    Open(char),
    Close(char),
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_ident(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == '-'
}

/// Cuts `text` into tokens. Comments are dropped; anything the tokenizer does not know is a [`Token::Delim`].
#[must_use]
pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let peek = |at: usize| chars.get(at).copied();
    while i < chars.len() {
        let c = chars[i];
        match c {
            '/' if peek(i + 1) == Some('*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && peek(i + 1) == Some('/')) {
                    i += 1;
                }
                i = (i + 2).min(chars.len());
            }
            c if c.is_whitespace() => {
                while i < chars.len() && chars[i].is_whitespace() {
                    i += 1;
                }
                out.push(Token::Whitespace);
            }
            '"' | '\'' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != quote {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                i += 1;
                out.push(Token::Str(s));
            }
            '#' if peek(i + 1).is_some_and(is_ident) => {
                i += 1;
                let start = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                out.push(Token::Hash(chars[start..i].iter().collect()));
            }
            '@' if peek(i + 1).is_some_and(|c| is_ident_start(c) || c == '-') => {
                i += 1;
                let start = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                out.push(Token::AtKeyword(chars[start..i].iter().collect()));
            }
            c if c.is_ascii_digit()
                || (c == '.' && peek(i + 1).is_some_and(|d| d.is_ascii_digit()))
                || ((c == '-' || c == '+')
                    && (peek(i + 1).is_some_and(|d| d.is_ascii_digit())
                        || (peek(i + 1) == Some('.')
                            && peek(i + 2).is_some_and(|d| d.is_ascii_digit())))) =>
            {
                let start = i;
                if c == '-' || c == '+' {
                    i += 1;
                }
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if peek(i) == Some('.') && peek(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                // An exponent: `1e3`, but not the unit `em`/`ex` of `2em`.
                if matches!(peek(i), Some('e' | 'E'))
                    && (peek(i + 1).is_some_and(|d| d.is_ascii_digit())
                        || (matches!(peek(i + 1), Some('+' | '-'))
                            && peek(i + 2).is_some_and(|d| d.is_ascii_digit())))
                {
                    i += 2;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let value: f32 = chars[start..i]
                    .iter()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0.0);
                let unit_start = i;
                if peek(i) == Some('%') {
                    i += 1;
                } else {
                    while i < chars.len()
                        && (chars[i].is_alphabetic() || chars[i] == '-' && i > unit_start)
                    {
                        i += 1;
                    }
                }
                out.push(Token::Number {
                    value,
                    unit: chars[unit_start..i]
                        .iter()
                        .collect::<String>()
                        .to_ascii_lowercase(),
                });
            }
            c if is_ident_start(c)
                || (c == '-' && peek(i + 1).is_some_and(|d| is_ident_start(d) || d == '-')) =>
            {
                let start = i;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                let name: String = chars[start..i].iter().collect();
                if peek(i) == Some('(') {
                    i += 1;
                    out.push(Token::Function(name));
                } else {
                    out.push(Token::Ident(name));
                }
            }
            ':' => {
                out.push(Token::Colon);
                i += 1;
            }
            ';' => {
                out.push(Token::Semicolon);
                i += 1;
            }
            ',' => {
                out.push(Token::Comma);
                i += 1;
            }
            '{' | '(' | '[' => {
                out.push(Token::Open(c));
                i += 1;
            }
            '}' | ')' | ']' => {
                out.push(Token::Close(c));
                i += 1;
            }
            c => {
                out.push(Token::Delim(c));
                i += 1;
            }
        }
    }
    out
}
