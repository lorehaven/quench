//! PDF objects and the parser for them (file structure and content streams share it).

use crate::error::{Error, Result};
use std::collections::BTreeMap;

pub type Dict = BTreeMap<String, Object>;

#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub dict: Dict,
    /// The bytes as stored, before any filter is undone.
    pub raw: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(String),
    String(Vec<u8>),
    Array(Vec<Self>),
    Dict(Dict),
    Stream(Box<Stream>),
    Ref(u32, u16),
    /// A bare keyword: an operator in a content stream (`Tj`, `BT`, ...).
    Op(String),
}

impl Object {
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(n) => Some(*n as f64),
            Self::Real(f) => Some(*f),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_name(&self) -> Option<&str> {
        match self {
            Self::Name(n) => Some(n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The dictionary of a dictionary or a stream.
    #[must_use]
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Self::Dict(d) => Some(d),
            Self::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }
}

const fn is_space(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

const fn is_regular(b: u8) -> bool {
    !is_space(b) && !is_delimiter(b)
}

const fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Reads objects from bytes.
pub struct Parser<'a> {
    data: &'a [u8],
    pub pos: usize,
    /// Whether `n g R` is a reference (file structure) or three operands (content streams).
    refs: bool,
}

const MAX_NESTING: usize = 64;

impl<'a> Parser<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8], pos: usize, refs: bool) -> Self {
        Self { data, pos, refs }
    }

    /// From here on, `n g R` is a reference.
    pub const fn refs_on(&mut self) {
        self.refs = true;
    }

    pub fn skip_space(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if is_space(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|&c| c != b'\n' && c != b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// The next object, or `None` at the end of the data.
    pub fn parse(&mut self) -> Result<Option<Object>> {
        self.object(0)
    }

    fn object(&mut self, depth: usize) -> Result<Option<Object>> {
        if depth > MAX_NESTING {
            return Err(Error::Malformed("objects nest too deeply".into()));
        }
        self.skip_space();
        let Some(b) = self.peek() else {
            return Ok(None);
        };
        let object = match b {
            b'/' => Object::Name(self.name()),
            b'(' => Object::String(self.literal_string()),
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                self.dictionary(depth)?
            }
            b'<' => Object::String(self.hex_string()),
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_space();
                    match self.peek() {
                        None => return Err(Error::Malformed("unterminated array".into())),
                        Some(b']') => {
                            self.pos += 1;
                            break;
                        }
                        _ => match self.object(depth + 1)? {
                            Some(o) => items.push(o),
                            None => return Err(Error::Malformed("unterminated array".into())),
                        },
                    }
                }
                Object::Array(items)
            }
            b']' | b')' | b'>' | b'{' | b'}' => {
                // A stray delimiter: skip it and treat it as an operator-like token.
                self.pos += 1;
                if b == b'>' && self.peek() == Some(b'>') {
                    self.pos += 1;
                    return Ok(Some(Object::Op(">>".into())));
                }
                Object::Op((b as char).to_string())
            }
            _ => self.keyword_or_number()?,
        };
        Ok(Some(object))
    }

    fn name(&mut self) -> String {
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(b) = self.peek() {
            if !is_regular(b) {
                break;
            }
            self.pos += 1;
            if b == b'#'
                && let (Some(h), Some(l)) = (
                    self.data.get(self.pos).and_then(|&c| hex_value(c)),
                    self.data.get(self.pos + 1).and_then(|&c| hex_value(c)),
                )
            {
                out.push(h << 4 | l);
                self.pos += 2;
            } else {
                out.push(b);
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn literal_string(&mut self) -> Vec<u8> {
        self.pos += 1;
        let mut out = Vec::new();
        let mut depth = 1;
        while let Some(b) = self.peek() {
            self.pos += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(e) = self.peek() else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut value = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek() {
                                    Some(d @ b'0'..=b'7') => {
                                        value = value * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(value as u8);
                        }
                        b'\r' => {
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn hex_string(&mut self) -> Vec<u8> {
        self.pos += 1;
        let mut out = Vec::new();
        let mut high: Option<u8> = None;
        while let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_value(b) {
                match high.take() {
                    Some(h) => out.push(h << 4 | v),
                    None => high = Some(v),
                }
            }
        }
        if let Some(h) = high {
            out.push(h << 4);
        }
        out
    }

    fn dictionary(&mut self, depth: usize) -> Result<Object> {
        let mut dict = Dict::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => return Err(Error::Malformed("unterminated dictionary".into())),
                Some(b'>') => {
                    self.pos += 1;
                    if self.peek() == Some(b'>') {
                        self.pos += 1;
                    }
                    break;
                }
                Some(b'/') => {
                    let key = self.name();
                    self.skip_space();
                    // A key with no value before the closing `>>` is dropped.
                    if self.peek() == Some(b'>') {
                        continue;
                    }
                    if let Some(value) = self.object(depth + 1)? {
                        dict.insert(key, value);
                    }
                }
                Some(_) => {
                    // Garbage between entries: skip one object.
                    self.object(depth + 1)?;
                }
            }
        }
        Ok(Object::Dict(dict))
    }

    fn keyword_or_number(&mut self) -> Result<Object> {
        let start = self.pos;
        while self.peek().is_some_and(is_regular) {
            self.pos += 1;
        }
        let word = &self.data[start..self.pos];
        if word.is_empty() {
            self.pos += 1;
            return Ok(Object::Null);
        }
        if let Some(number) = number(word) {
            if let Object::Int(n) = number
                && self.refs
                && n >= 0
                && let Some(reference) = self.reference(n)
            {
                return Ok(reference);
            }
            return Ok(number);
        }
        Ok(match word {
            b"true" => Object::Bool(true),
            b"false" => Object::Bool(false),
            b"null" => Object::Null,
            other => Object::Op(String::from_utf8_lossy(other).into_owned()),
        })
    }

    /// `generation R` after the object number `n`, consumed only if both are there.
    fn reference(&mut self, n: i64) -> Option<Object> {
        let save = self.pos;
        self.skip_space();
        let start = self.pos;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
        let generation: Option<u16> = std::str::from_utf8(&self.data[start..self.pos])
            .ok()
            .and_then(|s| s.parse().ok());
        if let Some(generation) = generation {
            self.skip_space();
            if self.peek() == Some(b'R')
                && !self.data.get(self.pos + 1).is_some_and(|&b| is_regular(b))
            {
                self.pos += 1;
                return Some(Object::Ref(u32::try_from(n).ok()?, generation));
            }
        }
        self.pos = save;
        None
    }
}

fn number(word: &[u8]) -> Option<Object> {
    let text = std::str::from_utf8(word).ok()?;
    let first = *word.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    if let Ok(n) = text.parse::<i64>() {
        return Some(Object::Int(n));
    }
    // PDF allows `-.5`, `4.`, and (from sloppy writers) `--5`.
    let trimmed = text.trim_start_matches(['+', '-']);
    let sign = if text.starts_with('-') { -1.0 } else { 1.0 };
    if trimmed.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && trimmed.bytes().any(|b| b.is_ascii_digit())
    {
        return trimmed.parse::<f64>().ok().map(|f| Object::Real(sign * f));
    }
    None
}
