//! CSS selectors: the subset used to pick elements out of a parsed document.
//!
//! Supported: type and `*`, `#id`, `.class`, attribute selectors (`[a]`, `[a=v]`, `~=`, `|=`, `^=`, `$=`,
//! `*=`, with an `i` flag), the combinators ` `, `>`, `+` and `~`, selector lists, and the pseudo-classes
//! `:first-child`, `:last-child`, `:only-child`, `:first-of-type`, `:last-of-type`, `:only-of-type`,
//! `:nth-child(…)`, `:nth-last-child(…)`, `:nth-of-type(…)`, `:nth-last-of-type(…)`, `:not(…)`, `:is(…)`,
//! `:where(…)`, `:empty` and `:root`.

use crate::dom::Node;
use std::fmt;

/// A selector that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError(String);

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid selector: {}", self.0)
    }
}

impl std::error::Error for SelectorError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    Exists,
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone)]
enum Simple {
    Type(String),
    Universal,
    Id(String),
    Class(String),
    Attribute {
        name: String,
        op: AttrOp,
        value: String,
        insensitive: bool,
    },
    /// `an+b` over the position among siblings (optionally only those of the same type), from the start or
    /// the end.
    Nth {
        a: i64,
        b: i64,
        of_type: bool,
        from_end: bool,
    },
    Not(Vec<Complex>),
    Is(Vec<Complex>),
    Empty,
    Root,
}

#[derive(Debug, Clone)]
struct Compound {
    parts: Vec<Simple>,
}

#[derive(Debug, Clone)]
struct Complex {
    /// Leftmost first; `combinators[i]` joins `compounds[i]` and `compounds[i + 1]`.
    compounds: Vec<Compound>,
    combinators: Vec<Combinator>,
}

/// A parsed selector list.
#[derive(Debug, Clone)]
pub struct Selector {
    alternatives: Vec<Complex>,
}

struct Reader<'a> {
    text: &'a str,
    pos: usize,
}

fn error<T>(message: impl Into<String>) -> Result<T, SelectorError> {
    Err(SelectorError(message.into()))
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '-' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || !c.is_ascii()
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) -> bool {
        let before = self.pos;
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
        self.pos > before
    }

    fn ident(&mut self) -> Result<String, SelectorError> {
        let start = self.pos;
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if c == '\\' {
                self.pos += 1;
                let Some(escaped) = self.peek() else { break };
                out.push(escaped);
                self.pos += escaped.len_utf8();
            } else if is_ident_char(c)
                && (self.pos > start || is_ident_start(c) || c.is_ascii_digit())
            {
                out.push(c);
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        if out.is_empty() {
            return error(format!("expected a name at {}", self.pos));
        }
        Ok(out)
    }

    fn string_or_ident(&mut self) -> Result<String, SelectorError> {
        match self.peek() {
            Some(q @ ('"' | '\'')) => {
                self.pos += 1;
                let mut out = String::new();
                loop {
                    let Some(c) = self.peek() else {
                        return error("unterminated string");
                    };
                    self.pos += c.len_utf8();
                    match c {
                        c if c == q => return Ok(out),
                        '\\' => {
                            if let Some(e) = self.peek() {
                                out.push(e);
                                self.pos += e.len_utf8();
                            }
                        }
                        c => out.push(c),
                    }
                }
            }
            _ => self.ident(),
        }
    }

    fn list(&mut self, nested: bool) -> Result<Vec<Complex>, SelectorError> {
        let mut out = Vec::new();
        loop {
            self.skip_space();
            out.push(self.complex()?);
            self.skip_space();
            if self.eat(',') {
                continue;
            }
            if nested && self.peek() == Some(')') {
                return Ok(out);
            }
            if self.peek().is_none() && !nested {
                return Ok(out);
            }
            return error(format!(
                "unexpected `{}`",
                self.peek().map_or(String::new(), String::from)
            ));
        }
    }

    fn complex(&mut self) -> Result<Complex, SelectorError> {
        let mut compounds = vec![self.compound()?];
        let mut combinators = Vec::new();
        loop {
            let had_space = self.skip_space();
            let combinator = if self.eat('>') {
                Combinator::Child
            } else if self.eat('+') {
                Combinator::Adjacent
            } else if self.eat('~') {
                Combinator::Sibling
            } else if had_space && !matches!(self.peek(), None | Some(',' | ')')) {
                Combinator::Descendant
            } else {
                return Ok(Complex {
                    compounds,
                    combinators,
                });
            };
            self.skip_space();
            combinators.push(combinator);
            compounds.push(self.compound()?);
        }
    }

    fn compound(&mut self) -> Result<Compound, SelectorError> {
        let mut parts = Vec::new();
        if self.eat('*') {
            parts.push(Simple::Universal);
        } else if self.peek().is_some_and(|c| is_ident_start(c) || c == '\\') {
            parts.push(Simple::Type(self.ident()?.to_ascii_lowercase()));
        }
        loop {
            match self.peek() {
                Some('#') => {
                    self.pos += 1;
                    parts.push(Simple::Id(self.ident()?));
                }
                Some('.') => {
                    self.pos += 1;
                    parts.push(Simple::Class(self.ident()?));
                }
                Some('[') => {
                    self.pos += 1;
                    parts.push(self.attribute()?);
                }
                Some(':') => {
                    self.pos += 1;
                    parts.push(self.pseudo()?);
                }
                _ => break,
            }
        }
        if parts.is_empty() {
            return error(format!("expected a selector at {}", self.pos));
        }
        Ok(Compound { parts })
    }

    fn attribute(&mut self) -> Result<Simple, SelectorError> {
        self.skip_space();
        let name = self.ident()?.to_ascii_lowercase();
        self.skip_space();
        let op = if self.eat(']') {
            return Ok(Simple::Attribute {
                name,
                op: AttrOp::Exists,
                value: String::new(),
                insensitive: false,
            });
        } else if self.eat('=') {
            AttrOp::Equals
        } else {
            let op = match self.peek() {
                Some('~') => AttrOp::Includes,
                Some('|') => AttrOp::DashMatch,
                Some('^') => AttrOp::Prefix,
                Some('$') => AttrOp::Suffix,
                Some('*') => AttrOp::Substring,
                _ => return error("bad attribute selector"),
            };
            self.pos += 1;
            if !self.eat('=') {
                return error("expected `=` in an attribute selector");
            }
            op
        };
        self.skip_space();
        let value = self.string_or_ident()?;
        self.skip_space();
        let mut insensitive = false;
        if matches!(self.peek(), Some('i' | 'I')) {
            self.pos += 1;
            insensitive = true;
            self.skip_space();
        } else if matches!(self.peek(), Some('s' | 'S')) {
            self.pos += 1;
            self.skip_space();
        }
        if !self.eat(']') {
            return error("expected `]`");
        }
        Ok(Simple::Attribute {
            name,
            op,
            value,
            insensitive,
        })
    }

    fn pseudo(&mut self) -> Result<Simple, SelectorError> {
        let name = self.ident()?.to_ascii_lowercase();
        let nth = |a, b, of_type, from_end| Simple::Nth {
            a,
            b,
            of_type,
            from_end,
        };
        let simple = match name.as_str() {
            "first-child" => nth(0, 1, false, false),
            "last-child" => nth(0, 1, false, true),
            "first-of-type" => nth(0, 1, true, false),
            "last-of-type" => nth(0, 1, true, true),
            "only-child" | "only-of-type" => {
                let of_type = name == "only-of-type";
                return Ok(Simple::Is(vec![Complex {
                    compounds: vec![Compound {
                        parts: vec![nth(0, 1, of_type, false), nth(0, 1, of_type, true)],
                    }],
                    combinators: Vec::new(),
                }]));
            }
            "empty" => Simple::Empty,
            "root" => Simple::Root,
            "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
                if !self.eat('(') {
                    return error(format!(":{name} needs an argument"));
                }
                let (a, b) = self.nth_argument()?;
                if !self.eat(')') {
                    return error("expected `)`");
                }
                nth(a, b, name.ends_with("of-type"), name.contains("last"))
            }
            "not" | "is" | "where" | "matches" => {
                if !self.eat('(') {
                    return error(format!(":{name} needs an argument"));
                }
                let list = self.list(true)?;
                if !self.eat(')') {
                    return error("expected `)`");
                }
                if name == "not" {
                    Simple::Not(list)
                } else {
                    Simple::Is(list)
                }
            }
            other => return error(format!("unsupported pseudo-class :{other}")),
        };
        Ok(simple)
    }

    /// `odd`, `even`, `3`, `2n+1`, `-n+3`, ...
    fn nth_argument(&mut self) -> Result<(i64, i64), SelectorError> {
        self.skip_space();
        let start = self.pos;
        while self.peek().is_some_and(|c| c != ')') {
            self.pos += 1;
        }
        let text: String = self.text[start..self.pos]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        match text.as_str() {
            "odd" => return Ok((2, 1)),
            "even" => return Ok((2, 0)),
            _ => {}
        }
        let bad = || SelectorError(format!("bad an+b expression `{text}`"));
        let Some(n_at) = text.find('n') else {
            return text.parse::<i64>().map(|b| (0, b)).map_err(|_| bad());
        };
        let a = match &text[..n_at] {
            "" | "+" => 1,
            "-" => -1,
            digits => digits.parse::<i64>().map_err(|_| bad())?,
        };
        let b = match &text[n_at + 1..] {
            "" => 0,
            rest => rest.parse::<i64>().map_err(|_| bad())?,
        };
        Ok((a, b))
    }
}

impl Selector {
    /// Parses a selector list such as `div.note > p:first-child, a[href^="http"]`.
    ///
    /// # Errors
    ///
    /// [`SelectorError`] for syntax this engine does not support or that is malformed.
    pub fn parse(text: &str) -> Result<Self, SelectorError> {
        let mut reader = Reader { text, pos: 0 };
        let alternatives = reader.list(false)?;
        Ok(Self { alternatives })
    }

    /// Whether the element matches any of the selectors in the list.
    #[must_use]
    pub fn matches(&self, node: Node<'_>) -> bool {
        node.is_element() && self.alternatives.iter().any(|c| matches_complex(c, node))
    }
}

fn matches_complex(complex: &Complex, node: Node<'_>) -> bool {
    matches_from(complex, complex.compounds.len() - 1, node)
}

/// Whether `node` matches compound `index` of the chain and everything to its left fits.
fn matches_from(complex: &Complex, index: usize, node: Node<'_>) -> bool {
    if !matches_compound(&complex.compounds[index], node) {
        return false;
    }
    if index == 0 {
        return true;
    }
    match complex.combinators[index - 1] {
        Combinator::Child => node
            .parent()
            .is_some_and(|p| p.is_element() && matches_from(complex, index - 1, p)),
        Combinator::Descendant => node
            .ancestors()
            .any(|p| p.is_element() && matches_from(complex, index - 1, p)),
        Combinator::Adjacent => node
            .prev_element()
            .is_some_and(|s| matches_from(complex, index - 1, s)),
        Combinator::Sibling => {
            let mut sibling = node.prev_element();
            while let Some(s) = sibling {
                if matches_from(complex, index - 1, s) {
                    return true;
                }
                sibling = s.prev_element();
            }
            false
        }
    }
}

fn matches_compound(compound: &Compound, node: Node<'_>) -> bool {
    compound.parts.iter().all(|part| matches_simple(part, node))
}

fn matches_simple(part: &Simple, node: Node<'_>) -> bool {
    match part {
        Simple::Universal => true,
        Simple::Type(name) => node.name() == Some(name.as_str()),
        Simple::Id(id) => node.attr("id") == Some(id.as_str()),
        Simple::Class(class) => node.has_class(class),
        Simple::Attribute {
            name,
            op,
            value,
            insensitive,
        } => node
            .attr(name)
            .is_some_and(|actual| attribute_matches(*op, actual, value, *insensitive)),
        Simple::Nth {
            a,
            b,
            of_type,
            from_end,
        } => {
            let mut position = 1;
            let mut sibling = if *from_end {
                node.next_element()
            } else {
                node.prev_element()
            };
            while let Some(s) = sibling {
                if !*of_type || s.name() == node.name() {
                    position += 1;
                }
                sibling = if *from_end {
                    s.next_element()
                } else {
                    s.prev_element()
                };
            }
            nth_matches(*a, *b, position)
        }
        Simple::Not(list) => !list.iter().any(|c| matches_complex(c, node)),
        Simple::Is(list) => list.iter().any(|c| matches_complex(c, node)),
        Simple::Empty => node.children().all(|c| c.is_comment()),
        Simple::Root => node.parent().is_some_and(|p| p.is_document()),
    }
}

fn attribute_matches(op: AttrOp, actual: &str, expected: &str, insensitive: bool) -> bool {
    let (actual, expected) = if insensitive {
        (actual.to_lowercase(), expected.to_lowercase())
    } else {
        (actual.to_string(), expected.to_string())
    };
    match op {
        AttrOp::Exists => true,
        AttrOp::Equals => actual == expected,
        AttrOp::Includes => {
            !expected.is_empty()
                && !expected.contains(char::is_whitespace)
                && actual.split_whitespace().any(|w| w == expected)
        }
        AttrOp::DashMatch => {
            actual == expected
                || actual
                    .strip_prefix(&expected)
                    .is_some_and(|r| r.starts_with('-'))
        }
        AttrOp::Prefix => !expected.is_empty() && actual.starts_with(&expected),
        AttrOp::Suffix => !expected.is_empty() && actual.ends_with(&expected),
        AttrOp::Substring => !expected.is_empty() && actual.contains(&expected),
    }
}

/// Whether position `p` (1-based) is `a·n + b` for some n ≥ 0.
const fn nth_matches(a: i64, b: i64, p: i64) -> bool {
    if a == 0 {
        return p == b;
    }
    let diff = p - b;
    diff % a == 0 && diff / a >= 0
}
