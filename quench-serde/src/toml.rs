//! TOML 1.0 to and from [`Value`].
//!
//! A table is a [`Value::Object`]. Dates and times have no `Value` form, so a
//! datetime is read as its RFC 3339 text (`1979-05-27T07:32:00Z`, or the local
//! date/time form) and can be read back into a [`quench_core::time::Time`]
//! when it has an offset. `inf` and `nan` read as non-finite floats (which JSON
//! cannot express, so they print as `null` there).
//!
//! Besides TOML 1.0 the parser accepts what 1.1 added: newlines and a trailing
//! comma inside inline tables, seconds optional in times, and the `\e` and
//! `\xHH` escapes.
//!
//! The parser enforces the table rules that make TOML strict: a key cannot be
//! defined twice, a `[table]` cannot be defined twice, a table made with dotted
//! keys cannot later get a `[header]`, and inline tables and arrays are closed
//! once written.

use std::fmt::Write as _;

use crate::{Deserialize, Error, Map, Result, Serialize, Value};

/// Reads a `T` from TOML text.
pub fn from_str<T: Deserialize>(text: &str) -> Result<T> {
    T::deserialize(&parse(text)?)
}

/// Parses a TOML document into an object.
pub fn parse(text: &str) -> Result<Value> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
    };
    let mut root = Table::new(State::Explicit);
    parser.document(&mut root)?;
    Ok(root.into_value())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Written as `[header]`, or the document root.
    Explicit,
    /// Created on the way to a deeper `[a.b.c]` header.
    Implicit,
    /// Created by a dotted key (`a.b = 1`).
    Dotted,
}

enum Node {
    Leaf(Value),
    Table(Table),
    /// `[[array.of.tables]]`
    Tables(Vec<Table>),
}

struct Table {
    entries: Vec<(String, Node)>,
    state: State,
}

impl Table {
    const fn new(state: State) -> Self {
        Self {
            entries: Vec::new(),
            state,
        }
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == key)
    }

    fn into_value(self) -> Value {
        Value::Object(
            self.entries
                .into_iter()
                .map(|(key, node)| {
                    let value = match node {
                        Node::Leaf(value) => value,
                        Node::Table(table) => table.into_value(),
                        Node::Tables(tables) => {
                            Value::Array(tables.into_iter().map(Self::into_value).collect())
                        }
                    };
                    (key, value)
                })
                .collect::<Map>(),
        )
    }
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

/// Why a statement could not be placed (turned into an error with a position).
type Placed<T> = std::result::Result<T, String>;

impl Parser<'_> {
    fn error(&self, message: impl std::fmt::Display) -> Error {
        Error::at(message, self.text, self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.pos += 1;
        }
        found
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// A `#` comment up to (not including) the line break.
    fn comment(&mut self) -> Result<()> {
        if self.peek() != Some(b'#') {
            return Ok(());
        }
        while let Some(byte) = self.peek() {
            match byte {
                b'\n' => break,
                b'\r' if self.bytes.get(self.pos + 1) == Some(&b'\n') => break,
                b if b != b'\t' && (b < 0x20 || b == 0x7f) => {
                    return Err(self.error("control character in comment"));
                }
                _ => self.pos += 1,
            }
        }
        Ok(())
    }

    fn newline(&mut self) -> bool {
        if self.eat(b'\n') {
            return true;
        }
        if self.peek() == Some(b'\r') && self.bytes.get(self.pos + 1) == Some(&b'\n') {
            self.pos += 2;
            return true;
        }
        false
    }

    /// Blank space, comments and line breaks between statements.
    fn skip_filler(&mut self) -> Result<()> {
        loop {
            self.skip_blanks();
            self.comment()?;
            if !self.newline() {
                return Ok(());
            }
        }
    }

    /// After a statement: only blanks and a comment may remain on the line.
    fn end_of_line(&mut self) -> Result<()> {
        self.skip_blanks();
        self.comment()?;
        if self.peek().is_none() || self.newline() {
            Ok(())
        } else {
            Err(self.error("expected a new line after the value"))
        }
    }

    fn document(&mut self, root: &mut Table) -> Result<()> {
        let mut current: Vec<String> = Vec::new();
        loop {
            self.skip_filler()?;
            match self.peek() {
                None => return Ok(()),
                Some(b'[') => {
                    let start = self.pos;
                    self.pos += 1;
                    let array = self.eat(b'[');
                    self.skip_blanks();
                    let path = self.key_path()?;
                    self.skip_blanks();
                    if !self.eat(b']') || (array && !self.eat(b']')) {
                        return Err(self.error("expected `]` closing the table header"));
                    }
                    self.end_of_line()?;
                    let placed = if array {
                        open_array_of_tables(root, &path)
                    } else {
                        open_table(root, &path)
                    };
                    placed.map_err(|m| Error::at(m, self.text, start))?;
                    current = path;
                }
                Some(_) => {
                    let start = self.pos;
                    let path = self.key_path()?;
                    self.skip_blanks();
                    if !self.eat(b'=') {
                        return Err(self.error("expected `=` after the key"));
                    }
                    self.skip_blanks();
                    let value = self.value()?;
                    self.end_of_line()?;
                    let table = current_table(root, &current);
                    insert_dotted(table, &path, value)
                        .map_err(|m| Error::at(m, self.text, start))?;
                }
            }
        }
    }

    /// `a.b."c d"` with optional blanks around the dots.
    fn key_path(&mut self) -> Result<Vec<String>> {
        let mut path = vec![self.key()?];
        loop {
            self.skip_blanks();
            if !self.eat(b'.') {
                return Ok(path);
            }
            self.skip_blanks();
            path.push(self.key()?);
        }
    }

    fn key(&mut self) -> Result<String> {
        match self.peek() {
            Some(b'"') => self.basic_string(false),
            Some(b'\'') => self.literal_string(false),
            Some(b) if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' => {
                let start = self.pos;
                while self
                    .peek()
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                {
                    self.pos += 1;
                }
                Ok(self.text[start..self.pos].to_string())
            }
            _ => Err(self.error("expected a key")),
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            None => Err(self.error("expected a value")),
            Some(b'"') => {
                let multi = self.bytes[self.pos..].starts_with(b"\"\"\"");
                self.basic_string(multi).map(Value::String)
            }
            Some(b'\'') => {
                let multi = self.bytes[self.pos..].starts_with(b"'''");
                self.literal_string(multi).map(Value::String)
            }
            Some(b'[') => self.array(),
            Some(b'{') => self.inline_table(),
            Some(_) => self.scalar(),
        }
    }

    fn array(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_filler()?;
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_filler()?;
            if self.eat(b',') {
                continue;
            }
            self.skip_filler()?;
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            return Err(self.error("expected `,` or `]` in the array"));
        }
    }

    fn inline_table(&mut self) -> Result<Value> {
        self.pos += 1;
        let mut table = Table::new(State::Explicit);
        loop {
            self.skip_filler()?;
            if self.eat(b'}') {
                return Ok(table.into_value());
            }
            let start = self.pos;
            let path = self.key_path()?;
            self.skip_filler()?;
            if !self.eat(b'=') {
                return Err(self.error("expected `=` after the key"));
            }
            self.skip_filler()?;
            let value = self.value()?;
            insert_dotted(&mut table, &path, value).map_err(|m| Error::at(m, self.text, start))?;
            self.skip_filler()?;
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(table.into_value());
            }
            return Err(self.error("expected `,` or `}` in the inline table"));
        }
    }

    fn literal_string(&mut self, multi: bool) -> Result<String> {
        let quote = if multi { 3 } else { 1 };
        self.pos += quote;
        if multi {
            self.newline();
        }
        let start = self.pos;
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            if multi {
                if self.bytes[self.pos..].starts_with(b"'''") {
                    // up to two more quotes may sit just before the closing three
                    let mut end = self.pos;
                    while end - self.pos < 2 && self.bytes.get(end + 3) == Some(&b'\'') {
                        end += 1;
                    }
                    let content = self.text[start..end].to_string();
                    self.pos = end + 3;
                    return self.check_text(&content, true).map(|()| content);
                }
            } else if byte == b'\'' {
                let content = self.text[start..self.pos].to_string();
                self.pos += 1;
                return self.check_text(&content, false).map(|()| content);
            } else if byte == b'\n' {
                return Err(self.error("a single-line string cannot contain a line break"));
            }
            self.pos += 1;
        }
    }

    /// No control characters except tab (and, in multi-line strings, line breaks).
    fn check_text(&self, content: &str, multi: bool) -> Result<()> {
        let bad = content.char_indices().find(|&(i, c)| {
            c != '\t'
                && (c < ' ' || c == '\u{7f}')
                && !(multi
                    && (c == '\n' || (c == '\r' && content.as_bytes().get(i + 1) == Some(&b'\n'))))
        });
        bad.map_or(Ok(()), |_| Err(self.error("control character in string")))
    }

    fn basic_string(&mut self, multi: bool) -> Result<String> {
        self.pos += if multi { 3 } else { 1 };
        if multi {
            self.newline();
        }
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("unterminated string"));
            };
            match byte {
                b'"' if !multi => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'"' if self.bytes[self.pos..].starts_with(b"\"\"\"") => {
                    // up to two extra quotes belong to the content
                    let mut extra = 0;
                    while extra < 2 && self.bytes.get(self.pos + 3 + extra) == Some(&b'"') {
                        extra += 1;
                    }
                    out.push_str(&"\"".repeat(extra));
                    self.pos += 3 + extra;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    self.escape(&mut out, multi)?;
                }
                b'\n' if !multi => {
                    return Err(self.error("a single-line string cannot contain a line break"));
                }
                b'\r' if multi && self.bytes.get(self.pos + 1) == Some(&b'\n') => {
                    out.push('\n');
                    self.pos += 2;
                }
                b'\n' => {
                    out.push('\n');
                    self.pos += 1;
                }
                b if b != b'\t' && (b < 0x20 || b == 0x7f) => {
                    return Err(self.error("control character in string"));
                }
                _ => {
                    let ch = self.text[self.pos..].chars().next().unwrap_or('\u{fffd}');
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String, multi: bool) -> Result<()> {
        let Some(byte) = self.peek() else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        let simple = match byte {
            b'b' => '\u{8}',
            b't' => '\t',
            b'n' => '\n',
            b'f' => '\u{c}',
            b'r' => '\r',
            b'"' => '"',
            b'\\' => '\\',
            b'e' => '\u{1b}',
            b'x' => return self.unicode(out, 2),
            b'u' => return self.unicode(out, 4),
            b'U' => return self.unicode(out, 8),
            // A line-ending backslash swallows the break and the whitespace after it.
            b' ' | b'\t' | b'\n' | b'\r' if multi => {
                self.pos -= 1;
                let mut look = self.pos;
                while matches!(self.bytes.get(look), Some(b' ' | b'\t')) {
                    look += 1;
                }
                let breaks = match (self.bytes.get(look), self.bytes.get(look + 1)) {
                    (Some(b'\n'), _) => 1,
                    (Some(b'\r'), Some(b'\n')) => 2,
                    _ => {
                        return Err(self.error(
                            "invalid escape: a backslash must end the line or start an escape",
                        ));
                    }
                };
                self.pos = look + breaks;
                while matches!(self.peek(), Some(b' ' | b'\t' | b'\n'))
                    || (self.peek() == Some(b'\r') && self.bytes.get(self.pos + 1) == Some(&b'\n'))
                {
                    self.pos += 1;
                }
                return Ok(());
            }
            _ => {
                self.pos -= 1;
                return Err(self.error("invalid escape sequence"));
            }
        };
        out.push(simple);
        Ok(())
    }

    fn unicode(&mut self, out: &mut String, digits: usize) -> Result<()> {
        let hex = self
            .bytes
            .get(self.pos..self.pos + digits)
            .ok_or_else(|| self.error("truncated unicode escape"))?;
        let mut code = 0u32;
        for &digit in hex {
            code = code * 16
                + char::from(digit)
                    .to_digit(16)
                    .ok_or_else(|| self.error("invalid unicode escape"))?;
        }
        let ch = char::from_u32(code)
            .ok_or_else(|| self.error("unicode escape is not a scalar value"))?;
        self.pos += digits;
        out.push(ch);
        Ok(())
    }

    /// Booleans, numbers and dates: whatever runs up to a delimiter.
    fn scalar(&mut self) -> Result<Value> {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if matches!(
                byte,
                b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}' | b'#'
            ) {
                break;
            }
            self.pos += 1;
        }
        // `1979-05-27 07:32:00` has a space inside the datetime.
        if self.pos - start == 10
            && is_date(&self.text[start..self.pos])
            && self.peek() == Some(b' ')
            && self.bytes.get(self.pos + 1).is_some_and(u8::is_ascii_digit)
        {
            self.pos += 1;
            while self.peek().is_some_and(|b| {
                !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b',' | b']' | b'}' | b'#')
            }) {
                self.pos += 1;
            }
        }
        let token = &self.text[start..self.pos];
        let fail = |what: &str| Error::at(format!("invalid {what}: `{token}`"), self.text, start);
        match token {
            "" => Err(Error::at("expected a value", self.text, start)),
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            "inf" | "+inf" => Ok(Value::Float(f64::INFINITY)),
            "-inf" => Ok(Value::Float(f64::NEG_INFINITY)),
            "nan" | "+nan" | "-nan" => Ok(Value::Float(f64::NAN)),
            _ => {
                if let Some(datetime) = datetime(token) {
                    return Ok(Value::String(datetime));
                }
                // Shaped like a date (`1979-..`) or time (`07:..`) but not a valid one.
                let bytes = token.as_bytes();
                let dated = bytes.len() >= 5
                    && bytes[..4].iter().all(u8::is_ascii_digit)
                    && bytes[4] == b'-';
                let timed = bytes.len() >= 3
                    && bytes[..2].iter().all(u8::is_ascii_digit)
                    && bytes[2] == b':';
                if dated || timed {
                    return Err(fail("date or time"));
                }
                number(token).ok_or_else(|| fail("value"))
            }
        }
    }
}

/// Resolves `[a.b.c]`'s parents, creating implicit tables.
fn walk<'t>(root: &'t mut Table, path: &[String]) -> Placed<&'t mut Table> {
    let mut table = root;
    for key in path {
        let index = table.position(key).unwrap_or_else(|| {
            table
                .entries
                .push((key.clone(), Node::Table(Table::new(State::Implicit))));
            table.entries.len() - 1
        });
        table = match &mut table.entries[index].1 {
            Node::Table(next) => next,
            Node::Tables(items) => items
                .last_mut()
                .ok_or_else(|| format!("`{key}` is an empty array of tables"))?,
            Node::Leaf(_) => return Err(format!("`{key}` is already defined as a value")),
        };
    }
    Ok(table)
}

fn open_table(root: &mut Table, path: &[String]) -> Placed<()> {
    let (last, parents) = path.split_last().ok_or("empty table header")?;
    let parent = walk(root, parents)?;
    match parent.position(last) {
        None => parent
            .entries
            .push((last.clone(), Node::Table(Table::new(State::Explicit)))),
        Some(i) => match &mut parent.entries[i].1 {
            Node::Table(table) if table.state == State::Implicit => table.state = State::Explicit,
            Node::Table(_) => return Err(format!("table `{last}` is defined more than once")),
            Node::Tables(_) => return Err(format!("`{last}` is an array of tables, not a table")),
            Node::Leaf(_) => return Err(format!("`{last}` is already defined as a value")),
        },
    }
    Ok(())
}

fn open_array_of_tables(root: &mut Table, path: &[String]) -> Placed<()> {
    let (last, parents) = path.split_last().ok_or("empty table header")?;
    let parent = walk(root, parents)?;
    match parent.position(last) {
        None => parent.entries.push((
            last.clone(),
            Node::Tables(vec![Table::new(State::Explicit)]),
        )),
        Some(i) => match &mut parent.entries[i].1 {
            Node::Tables(items) => items.push(Table::new(State::Explicit)),
            Node::Table(_) => return Err(format!("`{last}` is a table, not an array of tables")),
            Node::Leaf(_) => {
                return Err(format!(
                    "`{last}` is already defined as a value (arrays cannot be extended)"
                ));
            }
        },
    }
    Ok(())
}

/// The table `[header]` (or `[[header]]`'s latest element) currently open.
fn current_table<'t>(root: &'t mut Table, path: &[String]) -> &'t mut Table {
    // The headers that produced `path` were validated when they were opened.
    let mut table = root;
    for key in path {
        let index = table.position(key).expect("an open header exists");
        table = match &mut table.entries[index].1 {
            Node::Table(next) => next,
            Node::Tables(items) => items.last_mut().expect("arrays of tables are never empty"),
            Node::Leaf(_) => unreachable!("a header path never crosses a value"),
        };
    }
    table
}

/// `a.b.c = value` inside `table`.
fn insert_dotted(table: &mut Table, path: &[String], value: Value) -> Placed<()> {
    let (last, parents) = path.split_last().ok_or("empty key")?;
    let mut table = table;
    for key in parents {
        let index = table.position(key).unwrap_or_else(|| {
            table
                .entries
                .push((key.clone(), Node::Table(Table::new(State::Dotted))));
            table.entries.len() - 1
        });
        table = match &mut table.entries[index].1 {
            Node::Table(next) if next.state != State::Explicit => {
                // From here on it counts as defined by dotted keys.
                next.state = State::Dotted;
                next
            }
            Node::Table(_) => {
                return Err(format!(
                    "table `{key}` was defined with a header and cannot be extended with dotted keys"
                ));
            }
            Node::Tables(_) => {
                return Err(format!(
                    "`{key}` is an array of tables and cannot be extended with dotted keys"
                ));
            }
            Node::Leaf(_) => return Err(format!("`{key}` is already defined as a value")),
        };
    }
    if table.position(last).is_some() {
        return Err(format!("key `{last}` is defined more than once"));
    }
    table.entries.push((last.clone(), Node::Leaf(value)));
    Ok(())
}

/// `1_000`-style digits: underscores only between digits.
fn digits(text: &str, radix: u32) -> Option<String> {
    let mut previous_digit = false;
    let mut out = String::new();
    for c in text.chars() {
        if c == '_' {
            if !previous_digit {
                return None;
            }
            previous_digit = false;
        } else if c.is_digit(radix) {
            out.push(c);
            previous_digit = true;
        } else {
            return None;
        }
    }
    (previous_digit && !out.is_empty()).then_some(out)
}

fn number(token: &str) -> Option<Value> {
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(rest) = token.strip_prefix(prefix) {
            let n = i64::from_str_radix(&digits(rest, radix)?, radix).ok()?;
            return Some(Value::from_i64(n));
        }
    }
    let (negative, unsigned) = match token.as_bytes()[0] {
        b'-' => (true, &token[1..]),
        b'+' => (false, &token[1..]),
        _ => (false, token),
    };
    let is_float = unsigned.contains(['.', 'e', 'E']);
    let (int_part, rest) = unsigned
        .split_once(['.', 'e', 'E'])
        .map_or((unsigned, ""), |(i, _)| (i, &unsigned[i.len()..]));
    let int_digits = digits(int_part, 10)?;
    if int_digits.len() > 1 && int_digits.starts_with('0') {
        return None;
    }
    if !is_float {
        let n: i64 = format!("{}{int_digits}", if negative { "-" } else { "" })
            .parse()
            .ok()?;
        return Some(Value::from_i64(n));
    }
    let mut text = String::from(if negative { "-" } else { "" });
    text.push_str(&int_digits);
    let mut rest = rest;
    if let Some(fraction) = rest.strip_prefix('.') {
        let (frac, after) = fraction
            .split_once(['e', 'E'])
            .map_or((fraction, ""), |(f, _)| (f, &fraction[f.len()..]));
        text.push('.');
        text.push_str(&digits(frac, 10)?);
        rest = after;
    }
    if let Some(exponent) = rest.strip_prefix(['e', 'E']) {
        let (sign, body) = match exponent.as_bytes().first()? {
            b'-' => ("-", &exponent[1..]),
            b'+' => ("", &exponent[1..]),
            _ => ("", exponent),
        };
        text.push('e');
        text.push_str(sign);
        text.push_str(&digits(body, 10)?);
    } else if !rest.is_empty() {
        return None;
    }
    let f: f64 = text.parse().ok()?;
    f.is_finite().then_some(Value::Float(f))
}

fn two(text: &str, at: usize) -> Option<u32> {
    let digits = text.get(at..at + 2)?;
    digits
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| digits.parse().ok())
        .flatten()
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !text[..4].bytes().all(|b| b.is_ascii_digit())
    {
        return false;
    }
    let (Some(month), Some(day)) = (two(text, 5), two(text, 8)) else {
        return false;
    };
    let year: i32 = text[..4].parse().unwrap_or(0);
    quench_core::time::Time::from_ymd_hms(year, month, day, 0, 0, 0).is_some()
}

/// `HH:MM[:SS[.fraction]]`
fn is_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 5 || bytes[2] != b':' {
        return false;
    }
    let (Some(h), Some(m)) = (two(text, 0), two(text, 3)) else {
        return false;
    };
    if h > 23 || m > 59 {
        return false;
    }
    if bytes.len() == 5 {
        return true;
    }
    if bytes.len() < 8 || bytes[5] != b':' {
        return false;
    }
    let Some(s) = two(text, 6) else { return false };
    if s > 60 {
        return false;
    }
    match &text[8..] {
        "" => true,
        fraction => fraction
            .strip_prefix('.')
            .is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit())),
    }
}

/// A TOML date/time as normalised RFC 3339 text, or `None` if `token` is not one.
fn datetime(token: &str) -> Option<String> {
    if is_date(token) || is_time(token) {
        return Some(token.to_string());
    }
    let separator = token.find(['T', 't', ' '])?;
    let (date, rest) = (&token[..separator], &token[separator + 1..]);
    if !is_date(date) {
        return None;
    }
    let (time, offset) = rest
        .find(['Z', 'z', '+', '-'])
        .map_or((rest, None), |i| (&rest[..i], Some(&rest[i..])));
    if !is_time(time) {
        return None;
    }
    let offset = match offset {
        None => String::new(),
        Some("Z" | "z") => "Z".to_string(),
        Some(o) => {
            let valid = o.len() == 6
                && o.as_bytes()[3] == b':'
                && two(o, 1).is_some_and(|h| h < 24)
                && two(o, 4).is_some_and(|m| m < 60);
            if !valid {
                return None;
            }
            o.to_string()
        }
    };
    Some(format!("{date}T{time}{offset}"))
}

/// Writes a table as TOML. Arrays stay on one line: `a = [1, 2]`.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    write(&value.serialize(), false)
}

/// Like [`to_string`], with arrays one element per line and multi-line text
/// as `"""` strings.
pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    write(&value.serialize(), true)
}

fn write(value: &Value, pretty: bool) -> Result<String> {
    let Value::Object(root) = value else {
        return Err(Error::new(format!(
            "a TOML document is a table, found {}",
            value.type_name()
        )));
    };
    let mut out = String::new();
    write_table(&mut out, &mut Vec::new(), root, pretty, true)?;
    Ok(out)
}

fn is_array_of_tables(value: &Value) -> bool {
    matches!(value, Value::Array(items) if !items.is_empty() && items.iter().all(|i| matches!(i, Value::Object(_))))
}

fn write_table(
    out: &mut String,
    path: &mut Vec<String>,
    table: &Map,
    pretty: bool,
    header: bool,
) -> Result<()> {
    let direct: Vec<_> = table
        .iter()
        .filter(|(_, v)| !matches!(v, Value::Object(_)) && !is_array_of_tables(v) && !v.is_null())
        .collect();
    let has_children = table
        .values()
        .any(|v| matches!(v, Value::Object(_)) || is_array_of_tables(v));
    // A table with only sub-tables needs no header of its own.
    if header && !path.is_empty() && (!direct.is_empty() || !has_children) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push('[');
        out.push_str(&path_text(path));
        out.push_str("]\n");
    }
    for (key, value) in direct {
        out.push_str(&key_text(key));
        out.push_str(" = ");
        write_inline(out, value, pretty, 0).map_err(|e| e.in_field(key))?;
        out.push('\n');
    }
    for (key, value) in table {
        path.push(key.clone());
        match value {
            Value::Object(child) => {
                write_table(out, path, child, pretty, true).map_err(|e| e.in_field(key))?;
            }
            items if is_array_of_tables(items) => {
                for (i, item) in items.as_array().into_iter().flatten().enumerate() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str("[[");
                    out.push_str(&path_text(path));
                    out.push_str("]]\n");
                    if let Value::Object(child) = item {
                        write_table(out, path, child, pretty, false)
                            .map_err(|e| e.in_index(i).in_field(key))?;
                    }
                }
            }
            _ => {}
        }
        path.pop();
    }
    Ok(())
}

fn path_text(path: &[String]) -> String {
    path.iter()
        .map(|k| key_text(k))
        .collect::<Vec<_>>()
        .join(".")
}

fn key_text(key: &str) -> String {
    if !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        key.to_string()
    } else {
        string_text(key, false)
    }
}

fn string_text(text: &str, multiline: bool) -> String {
    if multiline
        && text.contains('\n')
        && !text.contains("\"\"\"")
        && !text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return format!("\"\"\"\n{}\"\"\"", text.replace('\\', "\\\\"));
    }
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn write_inline(out: &mut String, value: &Value, pretty: bool, depth: usize) -> Result<()> {
    match value {
        Value::Null => return Err(Error::new("TOML cannot represent null here")),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt(n) => {
            if i64::try_from(*n).is_err() {
                return Err(Error::new(format!(
                    "{n} is too large for a TOML integer (64-bit signed)"
                )));
            }
            let _ = write!(out, "{n}");
        }
        Value::Float(f) if f.is_nan() => out.push_str("nan"),
        Value::Float(f) if f.is_infinite() => out.push_str(if *f > 0.0 { "inf" } else { "-inf" }),
        Value::Float(f) => crate::json::write_float(out, *f),
        Value::String(s) => out.push_str(&string_text(s, pretty)),
        Value::Array(items) => {
            let spread = pretty && items.len() > 1;
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 && !spread {
                    out.push_str(", ");
                }
                if spread {
                    out.push('\n');
                    out.push_str(&"    ".repeat(depth + 1));
                }
                write_inline(out, item, pretty, depth + 1).map_err(|e| e.in_index(i))?;
                if spread {
                    out.push(',');
                }
            }
            if spread {
                out.push('\n');
                out.push_str(&"    ".repeat(depth));
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut first = true;
            for (key, item) in map {
                if item.is_null() {
                    continue;
                }
                out.push_str(if first { " " } else { ", " });
                first = false;
                out.push_str(&key_text(key));
                out.push_str(" = ");
                write_inline(out, item, false, depth + 1).map_err(|e| e.in_field(key))?;
            }
            out.push_str(if first { "}" } else { " }" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn ok(text: &str) -> Value {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    fn j(text: &str) -> Value {
        json::parse(text).unwrap()
    }

    fn rejects(text: &str) -> String {
        parse(text).expect_err(text).to_string()
    }

    #[quench_test::test]
    fn scalars() {
        let v = ok("s = \"x\"\ni = 42\nn = -7\nf = 1.5\nb = true\nc = false\nz = 0");
        assert_eq!(
            v,
            j(r#"{"s":"x","i":42,"n":-7,"f":1.5,"b":true,"c":false,"z":0}"#)
        );
    }

    #[quench_test::test]
    fn integers() {
        assert_eq!(
            ok(
                "a = +99\nb = 1_000\nc = 0xDEAD_beef\nd = 0o755\ne = 0b1101\nf = -0\ng = 9223372036854775807\nh = -9223372036854775808"
            ),
            j(
                r#"{"a":99,"b":1000,"c":3735928559,"d":493,"e":13,"f":0,"g":9223372036854775807,"h":-9223372036854775808}"#
            )
        );
        for bad in [
            "a = 01",
            "a = 1__0",
            "a = _1",
            "a = 1_",
            "a = 0x",
            "a = 0x_1",
            "a = -0x1",
            "a = 9223372036854775808",
            "a = 0xFFFFFFFFFFFFFFFF",
            "a = 1a",
            "a = --1",
            "a = 0o8",
            "a = 0b2",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn floats() {
        assert_eq!(
            ok(
                "a = 3.14\nb = +1.0\nc = -0.01\nd = 5e+22\ne = 1e06\nf = -2E-2\ng = 6.626e-34\nh = 1_0.0_1\ni = 6.6e-34"
            ),
            j(
                r#"{"a":3.14,"b":1.0,"c":-0.01,"d":5e22,"e":1000000.0,"f":-0.02,"g":6.626e-34,"h":10.01,"i":6.6e-34}"#
            )
        );
        assert!(matches!(ok("a = -0.0")["a"], Value::Float(f) if f == 0.0 && f.is_sign_negative()));
        for bad in [
            "a = 1.",
            "a = .5",
            "a = 1e",
            "a = 01.5",
            "a = 1.e2",
            "a = 1e400",
            "a = 1.5.2",
            "a = 1_.5",
            "a = 1e1_",
            "a = 1.5x",
            "a = e5",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn non_finite_floats() {
        let v = ok("a = inf\nb = +inf\nc = -inf\nd = nan\ne = -nan");
        assert_eq!(v["a"], Value::Float(f64::INFINITY));
        assert_eq!(v["b"], Value::Float(f64::INFINITY));
        assert_eq!(v["c"], Value::Float(f64::NEG_INFINITY));
        assert!(matches!(v["d"], Value::Float(f) if f.is_nan()));
        assert_eq!(
            to_string(&v).unwrap(),
            "a = inf\nb = inf\nc = -inf\nd = nan\ne = nan\n"
        );
    }

    #[quench_test::test]
    fn strings() {
        let v = ok(
            "a = \"hi\\n\\t\\\"\\\\\\u00e9\\U0001F600\\e\\x41\\b\\f\\r\"\nb = 'lit\\n\"q\"'\nc = \"\"",
        );
        assert_eq!(v["a"], Value::from("hi\n\t\"\\é😀\u{1b}A\u{8}\u{c}\r"));
        assert_eq!(v["b"], Value::from("lit\\n\"q\""));
        assert_eq!(v["c"], Value::from(""));
    }

    #[quench_test::test]
    fn multiline_strings() {
        assert_eq!(
            ok("a = \"\"\"\nline1\nline2\"\"\"")["a"],
            Value::from("line1\nline2")
        );
        assert_eq!(ok("a = \"\"\"\r\nx\r\ny\"\"\"")["a"], Value::from("x\ny"));
        assert_eq!(
            ok("a = \"\"\"one \\\n   two\\   \n\n  three\"\"\"")["a"],
            Value::from("one twothree")
        );
        assert_eq!(ok("a = \"\"\"q\"\"\"\"")["a"], Value::from("q\""));
        assert_eq!(
            ok("a = \"\"\"\"quoted\"\"\"\"\"")["a"],
            Value::from("\"quoted\"\"")
        );
        assert_eq!(ok("a = '''\nraw\\n\n'''")["a"], Value::from("raw\\n\n"));
        assert_eq!(ok("a = '''x'''''")["a"], Value::from("x''"));
        assert_eq!(ok("a = '''\r\nx'''")["a"], Value::from("x"));
        assert_eq!(
            ok("a = \"\"\"tab\there\"\"\"")["a"],
            Value::from("tab\there")
        );
        assert_eq!(ok("a = '''cr\r\nlf'''")["a"], Value::from("cr\r\nlf"));
    }

    #[quench_test::test]
    fn bad_strings() {
        for bad in [
            "a = \"abc",
            "a = \"a\nb\"",
            "a = 'a\nb'",
            "a = \"\\q\"",
            "a = \"\\u12\"",
            "a = \"\\uD800\"",
            "a = \"\\uzzzz\"",
            "a = \"\\U00110000\"",
            "a = \"\"\"a\\ b\"\"\"",
            "a = '''abc",
            "a = \"\"\"abc",
            "a = \"\\",
            "a = \"\u{7}\"",
            "a = 'x\u{1}'",
            "a = \"\u{7f}\"",
            "a = '''x\u{7}'''",
            "a = '''''''''",
            "a = \"abc\\",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(rejects("a = \"\\x4\"").contains("escape"));
    }

    #[quench_test::test]
    fn datetimes_are_kept_as_text() {
        let v = ok(
            "a = 1979-05-27T07:32:00Z\nb = 1979-05-27T07:32:00-07:00\nc = 1979-05-27 07:32:00Z\nd = 1979-05-27T07:32:00\ne = 1979-05-27T00:32:00.999999\nf = 1979-05-27\ng = 07:32:00\nh = 00:32:00.5\ni = 1979-05-27t07:32:00z\nj = 07:32\nk = 1979-05-27 07:32",
        );
        assert_eq!(v["a"], Value::from("1979-05-27T07:32:00Z"));
        assert_eq!(v["b"], Value::from("1979-05-27T07:32:00-07:00"));
        assert_eq!(v["c"], Value::from("1979-05-27T07:32:00Z"));
        assert_eq!(v["d"], Value::from("1979-05-27T07:32:00"));
        assert_eq!(v["e"], Value::from("1979-05-27T00:32:00.999999"));
        assert_eq!(v["f"], Value::from("1979-05-27"));
        assert_eq!(v["g"], Value::from("07:32:00"));
        assert_eq!(v["h"], Value::from("00:32:00.5"));
        assert_eq!(v["i"], Value::from("1979-05-27T07:32:00Z"));
        assert_eq!(v["j"], Value::from("07:32"));
        assert_eq!(v["k"], Value::from("1979-05-27T07:32"));
        // an offset datetime reads straight into Time
        let t: quench_core::time::Time = crate::from_value(&v["b"]).unwrap();
        assert_eq!(t.to_rfc3339(), "1979-05-27T14:32:00Z");
    }

    #[quench_test::test]
    fn bad_datetimes() {
        for bad in [
            "a = 1979-13-27",
            "a = 1979-02-30",
            "a = 1979-05-27T25:00:00Z",
            "a = 1979-05-27T07:32:00+25:00",
            "a = 1979-05-27T07:32:00+02:60",
            "a = 1979-05-27T07:32:00+0200",
            "a = 24:00:00",
            "a = 07:60:00",
            "a = 07:32:61",
            "a = 07:32:0",
            "a = 07:32:00.",
            "a = 1979-05-27T",
            "a = 1979-05-27 x",
            "a = 1979-5-27",
            "a = 07:3",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(rejects("a = 1979-13-27").contains("date or time"));
    }

    #[quench_test::test]
    fn arrays() {
        assert_eq!(ok("a = [1, 2, 3]")["a"], j("[1,2,3]"));
        assert_eq!(ok("a = [1, 2,]")["a"], j("[1,2]"));
        assert_eq!(ok("a = []")["a"], j("[]"));
        assert_eq!(
            ok("a = [\n  1, # one\n  # between\n  2\n]")["a"],
            j("[1,2]")
        );
        assert_eq!(
            ok("a = [1, \"two\", 3.0, [4], {b = 5}]")["a"],
            j(r#"[1,"two",3.0,[4],{"b":5}]"#)
        );
        assert_eq!(ok("a = [ [1, 2], [\"a\"] ]")["a"], j(r#"[[1,2],["a"]]"#));
        for bad in [
            "a = [,]",
            "a = [1,,2]",
            "a = [1 2]",
            "a = [1",
            "a = [",
            "a = [1, 2",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn inline_tables() {
        assert_eq!(ok("a = {b = 1, c = \"x\"}")["a"], j(r#"{"b":1,"c":"x"}"#));
        assert_eq!(ok("a = {}")["a"], j("{}"));
        assert_eq!(
            ok("a = {b.c = 1, b.d = 2}")["a"],
            j(r#"{"b":{"c":1,"d":2}}"#)
        );
        assert_eq!(ok("a = {b = 1,}")["a"], j(r#"{"b":1}"#));
        assert_eq!(
            ok("a = {\n  b = 1, # c\n  c =\n 2\n}")["a"],
            j(r#"{"b":1,"c":2}"#)
        );
        assert_eq!(
            ok("a = {b = {c = {d = 1}}}")["a"]["b"]["c"]["d"],
            Value::from(1)
        );
        for bad in [
            "a = {b = 1",
            "a = {b}",
            "a = {b = 1 c = 2}",
            "a = {b = 1, b = 2}",
            "a = {b = 1, b.c = 2}",
            "a = {,}",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn inline_tables_and_arrays_are_closed() {
        assert!(
            rejects("a = {b = 1}\na.c = 2").contains("cannot be extended")
                || rejects("a = {b = 1}\na.c = 2").contains("already")
        );
        assert!(parse("a = {b = 1}\n[a.c]\nd = 1").is_err());
        assert!(parse("a = [1]\n[[a]]\nb = 1").is_err());
        assert!(parse("a = [{b = 1}]\n[[a]]\nb = 2").is_err());
    }

    #[quench_test::test]
    fn keys() {
        assert_eq!(
            ok("\"quoted key\" = 1\n'lit key' = 2\n\"\" = 3\n1234 = 4\nbare-key_1 = 5"),
            j(r#"{"quoted key":1,"lit key":2,"":3,"1234":4,"bare-key_1":5}"#)
        );
        assert_eq!(ok("a . b . c = 1"), j(r#"{"a":{"b":{"c":1}}}"#));
        assert_eq!(ok("a.\"b.c\".d = 1"), j(r#"{"a":{"b.c":{"d":1}}}"#));
        assert_eq!(ok("a.b = 1\na.c = 2"), j(r#"{"a":{"b":1,"c":2}}"#));
        for bad in [
            "= 1",
            "a = ",
            "a 1",
            "a. = 1",
            ".a = 1",
            "a..b = 1",
            "a b = 1",
            "é = 1",
            "a = 1 b = 2",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn duplicate_keys_are_errors() {
        assert!(rejects("a = 1\na = 2").contains("more than once"));
        assert!(rejects("a.b = 1\na = 2").contains("more than once"));
        assert!(rejects("a = 1\na.b = 2").contains("already defined as a value"));
        assert!(parse("\"a\" = 1\na = 2").is_err());
    }

    #[quench_test::test]
    fn tables() {
        let v = ok("x = 1\n[a]\nb = 1\n[a.c]\nd = 2\n[e]\n");
        assert_eq!(v, j(r#"{"x":1,"a":{"b":1,"c":{"d":2}},"e":{}}"#));
        assert_eq!(ok("[ a . b ]\nc = 1"), j(r#"{"a":{"b":{"c":1}}}"#));
        assert_eq!(
            ok("[a.b.c]\nd = 1\n[a]\ne = 2"),
            j(r#"{"a":{"b":{"c":{"d":1}},"e":2}}"#)
        );
        assert_eq!(ok("[\"a b\"]\nc = 1"), j(r#"{"a b":{"c":1}}"#));
    }

    #[quench_test::test]
    fn table_redefinition_is_an_error() {
        assert!(rejects("[a]\nb = 1\n[a]\nc = 2").contains("more than once"));
        assert!(parse("[a]\nb = 1\n[a.b]\n").is_err());
        assert!(parse("[a]\nb.c = 1\n[a.b]\nd = 2").is_err());
        assert!(parse("a.b.c = 1\n[a]\n").is_err());
        assert!(parse("[a]\nb.c = 1\n[a.b]\nd = 2").is_err());
        assert!(parse("x = 1\n[x]\n").is_err());
        assert!(parse("[[a]]\n[a]\n").is_err());
        assert!(parse("[a]\n[[a]]\n").is_err());
        assert!(parse("x = 1\n[[x]]\n").is_err());
        assert!(parse("x = 1\n[x.y]\n").is_err());
        assert!(parse("[a]\nb = 1\n[a.b.c]\n").is_err());
    }

    #[quench_test::test]
    fn dotted_keys_can_gain_subtables_by_header() {
        let v = ok(
            "fruit.apple.color = \"red\"\nfruit.apple.taste.sweet = true\n[fruit.apple.texture]\nsmooth = true\n",
        );
        assert_eq!(v["fruit"]["apple"]["texture"]["smooth"], Value::Bool(true));
        assert!(parse("fruit.apple.color = \"red\"\n[fruit.apple]\n").is_err());
        assert!(parse("fruit.apple.color = \"red\"\n[fruit.apple.taste]\n").is_ok());
    }

    #[quench_test::test]
    fn arrays_of_tables() {
        let v = ok("[[p]]\nname = \"a\"\n[[p]]\n[[p]]\nname = \"c\"\ncolor = \"gray\"\n");
        assert_eq!(
            v,
            j(r#"{"p":[{"name":"a"},{},{"name":"c","color":"gray"}]}"#)
        );
        let nested =
            ok("[[a]]\nx = 1\n[[a.b]]\ny = 2\n[[a.b]]\ny = 3\n[[a]]\nx = 4\n[a.c]\nz = 5\n");
        assert_eq!(
            nested,
            j(r#"{"a":[{"x":1,"b":[{"y":2},{"y":3}]},{"x":4,"c":{"z":5}}]}"#)
        );
        assert_eq!(
            ok("[[a]]\nb.c = 1\n[[a]]\nb.c = 2")["a"][1]["b"]["c"],
            Value::from(2)
        );
        assert!(parse("[[a]]\nb = 1\n[a.b]\n").is_err());
        assert!(parse("[[a]]\n[[a.b]]\n[a]\n").is_err());
        // an implicit parent may be made explicit later, in a later element too
        assert!(parse("[[a]]\nb = 1\n[[a]]\n[a.b.c]\nd = 1\n[a.b]\nd = 1").is_ok());
        assert!(parse("[[a]]\nb.c = 1\n[[a]]\nb = 2\nb.d = 3").is_err());
        assert!(parse("a.b = 1\n[[a]]\n").is_err());
    }

    #[quench_test::test]
    fn comments_and_line_endings() {
        assert_eq!(ok("# only a comment"), j("{}"));
        assert_eq!(ok("# c\n[a] # c\nb = 1 # c\n"), j(r#"{"a":{"b":1}}"#));
        assert_eq!(ok("a = 1\r\nb = 2\r\n"), j(r#"{"a":1,"b":2}"#));
        assert_eq!(ok("a = 1\n\n\n\nb = 2"), j(r#"{"a":1,"b":2}"#));
        assert_eq!(ok(""), j("{}"));
        assert_eq!(ok("a = 1 # tab\there"), j(r#"{"a":1}"#));
        assert!(parse("a = 1\rb = 2").is_err());
        assert!(parse("# bell \u{7}\na = 1").is_err());
        assert!(parse("a = 1 # \u{7f}").is_err());
    }

    #[quench_test::test]
    fn bad_structure() {
        for bad in [
            "[a",
            "[a]]",
            "[]",
            "[a.]",
            "[.a]",
            "[a b]",
            "[[a]",
            "[[a]]]",
            "[a] x",
            "[a] = 1",
            "a = tru",
            "a = True",
            "a = 'x' 'y'",
            "a =\n1",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[quench_test::test]
    fn errors_have_positions() {
        let e = parse("a = 1\nb = oops\n").unwrap_err();
        assert_eq!(e.location(), Some((2, 5)));
        let e = parse("a = 1\n[x]\n[x]\n").unwrap_err();
        assert_eq!(e.location(), Some((3, 1)));
    }

    #[quench_test::test]
    fn spec_example() {
        let doc = "# This is a TOML document.\n\ntitle = \"TOML Example\"\n\n[owner]\nname = \"Tom Preston-Werner\"\ndob = 1979-05-27T07:32:00-08:00\n\n[database]\nenabled = true\nports = [ 8000, 8001, 8002 ]\ndata = [ [\"delta\", \"phi\"], [3.14] ]\ntemp_targets = { cpu = 79.5, case = 72.0 }\n\n[servers]\n\n[servers.alpha]\nip = \"10.0.0.1\"\nrole = \"frontend\"\n\n[servers.beta]\nip = \"10.0.0.2\"\nrole = \"backend\"\n";
        let v = ok(doc);
        assert_eq!(v["title"], Value::from("TOML Example"));
        assert_eq!(v["database"]["ports"][2], Value::from(8002));
        assert_eq!(v["database"]["temp_targets"]["case"], Value::Float(72.0));
        assert_eq!(v["servers"]["beta"]["role"], Value::from("backend"));
    }

    #[derive(Debug, PartialEq, crate::Serialize, crate::Deserialize)]
    struct Config {
        name: String,
        port: u16,
        #[serde(default)]
        tags: Vec<String>,
        db: Db,
        #[serde(default)]
        jobs: Vec<Job>,
    }

    #[derive(Debug, PartialEq, crate::Serialize, crate::Deserialize)]
    struct Db {
        url: String,
        pool: Option<u8>,
    }

    #[derive(Debug, PartialEq, crate::Serialize, crate::Deserialize)]
    struct Job {
        id: u32,
        steps: Vec<String>,
    }

    #[quench_test::test]
    fn typed_round_trip() {
        let config = Config {
            name: "svc".into(),
            port: 8080,
            tags: vec!["a".into(), "b".into()],
            db: Db {
                url: "postgres://x".into(),
                pool: None,
            },
            jobs: vec![
                Job {
                    id: 1,
                    steps: vec!["build".into()],
                },
                Job {
                    id: 2,
                    steps: vec![],
                },
            ],
        };
        let text = to_string(&config).unwrap();
        assert_eq!(
            text,
            "name = \"svc\"\nport = 8080\ntags = [\"a\", \"b\"]\n\n[db]\nurl = \"postgres://x\"\n\n[[jobs]]\nid = 1\nsteps = [\"build\"]\n\n[[jobs]]\nid = 2\nsteps = []\n"
        );
        assert_eq!(from_str::<Config>(&text).unwrap(), config);
        let pretty = to_string_pretty(&config).unwrap();
        assert!(
            pretty.contains("tags = [\n    \"a\",\n    \"b\",\n]"),
            "{pretty}"
        );
        assert_eq!(from_str::<Config>(&pretty).unwrap(), config);
    }

    #[quench_test::test]
    fn typed_errors_name_the_path() {
        let e = from_str::<Config>("name = \"x\"\nport = 1\n[db]\nurl = 5\n").unwrap_err();
        assert_eq!(e.path(), "db.url");
        assert!(from_str::<Config>("name = \"x\"").is_err());
    }

    #[quench_test::test]
    fn writer_layouts() {
        let v = j(
            r#"{"top":1,"sub":{"a":1,"deep":{"b":2}},"only_children":{"x":{"y":1}},"empty":{},"list":[{"n":1},{"n":2}],"mixed":[1,{"k":2}],"nested":[{"inner":{"z":1}}]}"#,
        );
        let text = to_string(&v).unwrap();
        assert!(
            text.starts_with("top = 1\nmixed = [1, { k = 2 }]\n"),
            "{text}"
        );
        assert!(
            text.contains("\n[sub]\na = 1\n\n[sub.deep]\nb = 2\n"),
            "{text}"
        );
        assert!(text.contains("[only_children.x]\ny = 1"), "{text}");
        assert!(!text.contains("[only_children]"), "{text}");
        assert!(text.contains("\n[empty]\n"), "{text}");
        assert!(
            text.contains("[[list]]\nn = 1\n\n[[list]]\nn = 2\n"),
            "{text}"
        );
        assert!(
            text.contains("[[nested]]\n\n[nested.inner]\nz = 1\n")
                || text.contains("[[nested]]\n[nested.inner]"),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), v);
    }

    #[quench_test::test]
    fn writer_quotes_keys_and_escapes_strings() {
        let v = j(r#"{"plain":"x","with space":"a\"b\\c\n\t\u0001\u007f","":1,"é":2}"#);
        let text = to_string(&v).unwrap();
        assert!(
            text.contains("\"with space\" = \"a\\\"b\\\\c\\n\\t\\u0001\\u007F\""),
            "{text}"
        );
        assert!(text.contains("\"\" = 1"), "{text}");
        assert_eq!(parse(&text).unwrap(), v);
    }

    #[quench_test::test]
    fn pretty_uses_multiline_strings_only_when_safe() {
        let v = j(
            r#"{"a":"one\ntwo","b":"has \"\"\" quotes\nx","c":"back\\slash\nx","d":"ctl\u0001\nx"}"#,
        );
        let text = to_string_pretty(&v).unwrap();
        assert!(text.contains("a = \"\"\"\none\ntwo\"\"\""), "{text}");
        assert!(
            text.contains("b = \"has \\\"\\\"\\\" quotes\\nx\""),
            "{text}"
        );
        assert!(
            text.contains("c = \"\"\"\nback\\\\slash\nx\"\"\""),
            "{text}"
        );
        assert!(text.contains("d = \"ctl\\u0001\\nx\""), "{text}");
        assert_eq!(parse(&text).unwrap(), v);
    }

    #[quench_test::test]
    fn writer_floats() {
        let text = to_string(&j(r#"{"a":1.0,"b":-0.5,"c":1e20,"d":5e-324}"#)).unwrap();
        assert_eq!(text, "a = 1.0\nb = -0.5\nc = 1e+20\nd = 5e-324\n");
    }

    #[quench_test::test]
    fn writer_nulls() {
        // a null table entry is omitted, as an unset Option is
        assert_eq!(
            to_string(&j(r#"{"a":1,"n":null,"t":{"x":null}}"#)).unwrap(),
            "a = 1\n\n[t]\n"
        );
        assert_eq!(
            to_string(&j(r#"{"a":{"b":null,"c":1}}"#)).unwrap(),
            "[a]\nc = 1\n"
        );
        let inline = to_string(&j(r#"{"a":[{"b":null,"c":1},2]}"#)).unwrap();
        assert_eq!(inline, "a = [{ c = 1 }, 2]\n");
        let e = to_string(&j(r#"{"a":[1,null]}"#)).unwrap_err();
        assert_eq!(e.to_string(), "a[1]: TOML cannot represent null here");
        assert_eq!(to_string(&j(r#"{"a":[{}]}"#)).unwrap(), "[[a]]\n");
    }

    #[quench_test::test]
    fn writer_limits() {
        assert!(
            to_string(&j("[1]"))
                .unwrap_err()
                .to_string()
                .contains("is a table")
        );
        let big = Value::Object(std::iter::once(("n", Value::UInt(u64::MAX))).collect());
        assert!(
            to_string(&big)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
        assert!(
            to_string(&j(r#"{"a":{"b":{"c":[1,null]}}}"#))
                .unwrap_err()
                .path()
                .starts_with("a.b.c")
        );
        assert!(
            to_string(&j(r#"{"l":[{"a":[null]}]}"#))
                .unwrap_err()
                .path()
                .starts_with("l[0]")
        );
    }

    #[quench_test::test]
    fn writer_round_trips_awkward_values() {
        let v = j(
            r#"{"a":{"b.c":{"d e":[1,2,[3,[4]]]}},"s":["x","y"],"t":[{"u":[{"v":1}]}],"i":-9223372036854775808}"#,
        );
        for pretty in [false, true] {
            let text = if pretty {
                to_string_pretty(&v)
            } else {
                to_string(&v)
            }
            .unwrap();
            assert_eq!(parse(&text).unwrap(), v, "{text}");
        }
    }
}
