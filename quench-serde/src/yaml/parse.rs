//! The YAML reader: block and flow styles over [`Value`].

use std::collections::HashMap;

use crate::{Error, Map, Result, Value};

use super::resolve;

/// Parses one document; an empty stream is `null`, several documents an error.
pub fn parse(text: &str) -> Result<Value> {
    let mut documents = parse_all(text)?;
    match documents.len() {
        0 => Ok(Value::Null),
        1 => Ok(documents.remove(0)),
        _ => Err(Error::new(
            "expected a single YAML document, found several (use parse_all)",
        )),
    }
}

/// Parses every document in a stream.
pub fn parse_all(text: &str) -> Result<Vec<Value>> {
    // A byte-order mark may open the stream.
    let skip = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: skip,
        anchors: HashMap::new(),
        first_key: None,
        depth: 0,
    };
    parser.stream()
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    anchors: HashMap<String, Value>,
    /// The key that opened the block mapping just parsed, so that properties
    /// written before `key: value` on one line can attach to the key.
    first_key: Option<String>,
    /// Nodes currently open, block or flow.
    depth: usize,
}

/// How deeply nodes may nest, so hostile input cannot exhaust the stack (the same cap as
/// `json::MAX_DEPTH`).
const MAX_DEPTH: usize = 128;

/// How a scalar ended.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    /// At a `:` that makes the scalar a mapping key.
    Key,
    /// At a line break, a comment, a flow indicator or the end of input.
    Other,
}

const fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

const fn is_break(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

impl Parser<'_> {
    fn error(&self, message: impl std::fmt::Display) -> Error {
        Error::at(message, self.text, self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn line_start(&self) -> usize {
        self.bytes[..self.pos]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1)
    }

    fn column(&self) -> usize {
        self.text[self.line_start()..self.pos].chars().count()
    }

    fn skip_blanks(&mut self) {
        while self.peek().is_some_and(is_blank) {
            self.pos += 1;
        }
    }

    /// Nothing but a comment or a line break (or the end) before the next line.
    fn at_line_end(&self) -> bool {
        let mut at = self.pos;
        while self.bytes.get(at).copied().is_some_and(is_blank) {
            at += 1;
        }
        match self.bytes.get(at) {
            None => true,
            Some(&b) if is_break(b) => true,
            // A `#` starts a comment only at the start of a line or after a blank.
            Some(b'#') => at == 0 || is_blank(self.bytes[at - 1]) || is_break(self.bytes[at - 1]),
            Some(_) => false,
        }
    }

    fn skip_line_break(&mut self) {
        if self.peek() == Some(b'\r') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
    }

    /// Past blanks, comments and line breaks to the next content; its column.
    fn skip_to_content(&mut self) -> Result<Option<usize>> {
        loop {
            let at_line_start = self.pos == 0 || self.bytes[self.pos - 1] == b'\n';
            let before = self.pos;
            self.skip_blanks();
            match self.peek() {
                None => return Ok(None),
                Some(b'#')
                    if at_line_start
                        || self.pos > before
                        || self.bytes[self.pos - 1].is_ascii_whitespace() =>
                {
                    while self.peek().is_some_and(|b| !is_break(b)) {
                        self.pos += 1;
                    }
                }
                Some(b) if is_break(b) => self.skip_line_break(),
                Some(_) => {
                    if at_line_start && self.bytes[before..self.pos].contains(&b'\t') {
                        self.pos = before
                            + self.bytes[before..self.pos]
                                .iter()
                                .position(|&b| b == b'\t')
                                .unwrap_or(0);
                        return Err(self.error("tabs cannot be used for indentation"));
                    }
                    return Ok(Some(self.column()));
                }
            }
        }
    }

    /// `---` or `...` at the start of a line, followed by a blank or the end.
    fn document_marker(&self) -> Option<&'static str> {
        if self.pos != self.line_start() {
            return None;
        }
        let rest = &self.bytes[self.pos..];
        let marker = if rest.starts_with(b"---") {
            "---"
        } else if rest.starts_with(b"...") {
            "..."
        } else {
            return None;
        };
        rest.get(3)
            .is_none_or(|&b| is_blank(b) || is_break(b))
            .then_some(marker)
    }

    fn stream(&mut self) -> Result<Vec<Value>> {
        let mut documents = Vec::new();
        loop {
            self.anchors.clear();
            if self.skip_to_content()?.is_none() {
                return Ok(documents);
            }
            // directives belong to the next document
            while self.pos == self.line_start() && self.peek() == Some(b'%') {
                while self.peek().is_some_and(|b| !is_break(b)) {
                    self.pos += 1;
                }
                if self.skip_to_content()?.is_none() {
                    return Ok(documents);
                }
            }
            let explicit = self.document_marker() == Some("---");
            if self.document_marker() == Some("...") {
                self.pos += 3;
                continue;
            }
            let value = if explicit {
                self.pos += 3;
                self.skip_blanks();
                if self.at_line_end() {
                    self.block_node(-1, false)?
                } else {
                    self.node_at(-1, false, false)?
                }
            } else {
                self.node_at(-1, false, false)?
            };
            documents.push(value);
            match self.skip_to_content()? {
                None => return Ok(documents),
                Some(_) => match self.document_marker() {
                    Some("...") => self.pos += 3,
                    Some(_) => {}
                    None => return Err(self.error("unexpected content after the document")),
                },
            }
        }
    }

    /// A node that starts on a following line (or is absent).
    fn block_node(&mut self, parent: isize, sequence_at_parent_column: bool) -> Result<Value> {
        let Some(column) = self.skip_to_content()? else {
            return Ok(Value::Null);
        };
        if self.document_marker().is_some() {
            return Ok(Value::Null);
        }
        let column = isize::try_from(column).unwrap_or(isize::MAX);
        let dash =
            self.peek() == Some(b'-') && self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b));
        if column > parent || (sequence_at_parent_column && dash && column == parent) {
            self.node_at(parent, false, sequence_at_parent_column)
        } else {
            Ok(Value::Null)
        }
    }

    /// The node starting at the cursor. `parent` is the column of the block
    /// that contains it; `inline` forbids block collections (a mapping value
    /// written on its key's line).
    fn node_at(
        &mut self,
        parent: isize,
        inline: bool,
        sequence_at_parent_column: bool,
    ) -> Result<Value> {
        self.nested(|this| this.node_at_inner(parent, inline, sequence_at_parent_column))
    }

    /// Runs a node parser one level deeper, refusing past [`MAX_DEPTH`].
    fn nested<T>(&mut self, parse: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error("recursion limit exceeded"));
        }
        self.depth += 1;
        let value = parse(self);
        self.depth -= 1;
        value
    }

    fn node_at_inner(
        &mut self,
        parent: isize,
        inline: bool,
        sequence_at_parent_column: bool,
    ) -> Result<Value> {
        let mut anchor = None;
        let mut tag = None;
        loop {
            match self.peek() {
                Some(b'&') => anchor = Some(self.name()?),
                Some(b'!') => tag = Some(self.tag()),
                _ => break,
            }
            self.skip_blanks();
        }
        let has_properties = anchor.is_some() || tag.is_some();
        let on_own_line = has_properties && self.at_line_end();
        self.first_key = None;
        let (value, plain_text) = if on_own_line {
            let value = self.block_node(parent, sequence_at_parent_column)?;
            self.first_key = None;
            (value, None)
        } else {
            self.node_body(parent, inline)?
        };
        // `&a key: value` on one line labels the key, not the mapping.
        if has_properties
            && !on_own_line
            && let Some(key) = self.first_key.take()
        {
            if let Some(name) = anchor {
                self.anchors.insert(name, Value::String(key));
            }
            return Ok(value);
        }
        let value = apply_tag(tag.as_deref(), value, plain_text.as_deref());
        if let Some(name) = anchor {
            self.anchors.insert(name, value.clone());
        }
        Ok(value)
    }

    /// Anchor and alias names run to the next blank or flow indicator.
    fn name(&mut self) -> Result<String> {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|b| {
            !is_blank(b) && !is_break(b) && !matches!(b, b',' | b'[' | b']' | b'{' | b'}')
        }) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("an anchor or alias needs a name"));
        }
        Ok(self.text[start..self.pos].to_string())
    }

    fn tag(&mut self) -> String {
        let start = self.pos;
        while self.peek().is_some_and(|b| {
            !is_blank(b) && !is_break(b) && !matches!(b, b',' | b'[' | b']' | b'{' | b'}')
        }) {
            self.pos += 1;
        }
        self.text[start..self.pos].to_string()
    }

    /// The value at the cursor, plus the raw text if it was a plain scalar
    /// (a `!!str` tag wants the text, not the typed value).
    fn node_body(&mut self, parent: isize, inline: bool) -> Result<(Value, Option<String>)> {
        let column = self.column();
        let here = isize::try_from(column).unwrap_or(isize::MAX);
        match self.peek() {
            None => Ok((Value::Null, None)),
            Some(b'*') => {
                let name = self.name()?;
                let value = self
                    .anchors
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| self.error(format!("unknown anchor `{name}`")))?;
                self.skip_blanks();
                if self.peek() == Some(b':')
                    && self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b))
                {
                    return Err(self.error("an alias cannot be a mapping key here"));
                }
                Ok((value, None))
            }
            Some(b'-') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                if inline {
                    return Err(
                        self.error("block sequence entries are not allowed in this context")
                    );
                }
                Ok((self.block_sequence(column)?, None))
            }
            Some(b'?') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                Err(self.error("complex mapping keys (`? `) are not supported"))
            }
            Some(b'|' | b'>') => Ok((self.block_scalar(parent)?, None)),
            Some(b'[' | b'{') => {
                let value = self.flow(parent)?;
                self.skip_blanks();
                if self.peek() == Some(b':')
                    && self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b))
                {
                    return Err(self.error("a collection cannot be a mapping key here"));
                }
                Ok((value, None))
            }
            Some(b'%' | b'@' | b'`') => {
                Err(self.error("this character cannot start a value; quote the text"))
            }
            Some(b',' | b']' | b'}') => {
                Err(self.error("this indicator cannot start a value; quote the text"))
            }
            Some(b':') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                Err(self.error("a mapping entry needs a key before `:`"))
            }
            Some(b'"' | b'\'') => {
                let quote = self.bytes[self.pos];
                let text = if quote == b'"' {
                    self.double_quoted(parent)?
                } else {
                    self.single_quoted(parent)?
                };
                self.after_scalar(text, true, column, inline)
            }
            Some(_) => {
                let (text, end) = self.plain(parent, false)?;
                if end == End::Key {
                    return self.finish_key(text, here, inline);
                }
                Ok((resolve::plain(&text), Some(text)))
            }
        }
    }

    /// After a quoted scalar: is it a key (`"k": v`) or a value?
    fn after_scalar(
        &mut self,
        text: String,
        quoted: bool,
        column: usize,
        inline: bool,
    ) -> Result<(Value, Option<String>)> {
        let save = self.pos;
        self.skip_blanks();
        if self.peek() == Some(b':') && self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) {
            return self.finish_key(text, isize::try_from(column).unwrap_or(isize::MAX), inline);
        }
        self.pos = save;
        let _ = quoted;
        Ok((Value::String(text), None))
    }

    fn finish_key(
        &mut self,
        key: String,
        column: isize,
        inline: bool,
    ) -> Result<(Value, Option<String>)> {
        if inline {
            return Err(self.error("mapping values are not allowed in this context"));
        }
        self.skip_blanks();
        let first = key.clone();
        let mapping = self.block_mapping(column, key)?;
        self.first_key = Some(first);
        Ok((mapping, None))
    }

    fn block_sequence(&mut self, column: usize) -> Result<Value> {
        let parent = isize::try_from(column).unwrap_or(isize::MAX);
        let mut items = Vec::new();
        loop {
            self.pos += 1; // the dash
            self.skip_blanks();
            let item = if self.at_line_end() {
                self.block_node(parent, false)?
            } else {
                self.node_at(parent, false, false)?
            };
            items.push(item);
            match self.skip_to_content()? {
                Some(next) if self.document_marker().is_none() => {
                    let dash = self.peek() == Some(b'-')
                        && self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b));
                    if next == column && dash {
                        continue;
                    }
                    if next > column {
                        return Err(self.error("bad indentation of a sequence entry"));
                    }
                    return Ok(Value::Array(items));
                }
                _ => return Ok(Value::Array(items)),
            }
        }
    }

    fn block_mapping(&mut self, column: isize, first_key: String) -> Result<Value> {
        let mut map = Map::new();
        let mut key = first_key;
        loop {
            // `:` then the value
            if self.peek() != Some(b':') {
                return Err(self.error("expected `:` after the mapping key"));
            }
            self.pos += 1;
            self.skip_blanks();
            let value = if self.at_line_end() {
                self.block_node(column, true)?
            } else {
                self.node_at(column, true, true)?
            };
            if map.contains_key(&key) {
                return Err(self.error(format!("duplicate mapping key `{key}`")));
            }
            map.insert(key, value);

            let Some(next) = self.skip_to_content()? else {
                return Ok(Value::Object(map));
            };
            if self.document_marker().is_some() {
                return Ok(Value::Object(map));
            }
            let next = isize::try_from(next).unwrap_or(isize::MAX);
            if next < column {
                return Ok(Value::Object(map));
            }
            if next > column {
                return Err(self.error("bad indentation of a mapping entry"));
            }
            key = self.mapping_key()?;
        }
    }

    /// A key at the start of a mapping line, cursor left on its `:`.
    fn mapping_key(&mut self) -> Result<String> {
        match self.peek() {
            Some(b'"') => {
                let key = self.double_quoted(-1)?;
                self.skip_blanks();
                Ok(key)
            }
            Some(b'\'') => {
                let key = self.single_quoted(-1)?;
                self.skip_blanks();
                Ok(key)
            }
            Some(b'-') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                Err(self.error("a sequence entry is not allowed where a mapping key is expected"))
            }
            Some(b'?') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                Err(self.error("complex mapping keys (`? `) are not supported"))
            }
            Some(b'*') => Err(self.error("an alias cannot be a mapping key here")),
            Some(b':') if self.peek_at(1).is_none_or(|b| is_blank(b) || is_break(b)) => {
                Err(self.error("a mapping entry needs a key before `:`"))
            }
            Some(b'&' | b'!') => {
                // properties on a key are accepted and ignored
                while matches!(self.peek(), Some(b'&' | b'!')) {
                    self.tag();
                    self.skip_blanks();
                }
                self.mapping_key()
            }
            _ => {
                let (text, end) = self.plain(-1, true)?;
                if end == End::Key {
                    Ok(text)
                } else {
                    Err(self.error("could not find the expected `:` after the mapping key"))
                }
            }
        }
    }

    /// A plain scalar. Stops at `: `, ` #`, a flow indicator (in flow) or a
    /// line break; in block context it continues onto more-indented lines.
    fn plain(&mut self, parent: isize, key_only: bool) -> Result<(String, End)> {
        self.plain_in(parent, key_only, false)
    }

    fn plain_in(&mut self, parent: isize, key_only: bool, flow: bool) -> Result<(String, End)> {
        let (mut text, mut end) = self.plain_line(flow)?;
        if end == End::Key || key_only {
            return Ok((text, end));
        }
        // Continuation lines fold into the scalar.
        let mut resume = self.pos;
        loop {
            let mut look = self.pos;
            let mut blank_lines = 0usize;
            // move over the line break and any blank lines
            if look >= self.bytes.len() {
                break;
            }
            loop {
                let mut at = look;
                while self.bytes.get(at).copied().is_some_and(is_blank) {
                    at += 1;
                }
                match self.bytes.get(at) {
                    Some(&b) if is_break(b) => {
                        blank_lines += 1;
                        look = at + 1;
                        if b == b'\r' && self.bytes.get(look) == Some(&b'\n') {
                            look += 1;
                        }
                    }
                    _ => break,
                }
            }
            // `look` is the start of the next non-blank line
            // the first break ended the current line
            blank_lines = blank_lines.saturating_sub(1);
            let mut content = look;
            while self.bytes.get(content).copied().is_some_and(|b| b == b' ') {
                content += 1;
            }
            let next_column =
                isize::try_from(self.text[look..content].chars().count()).unwrap_or(isize::MAX);
            // tabs may separate, but not indent
            while self.bytes.get(content).copied().is_some_and(is_blank) {
                content += 1;
            }
            let first = self.bytes.get(content).copied();
            let marker = look == content
                && (self.bytes[look..].starts_with(b"---")
                    || self.bytes[look..].starts_with(b"..."))
                && self
                    .bytes
                    .get(look + 3)
                    .is_none_or(|&b| is_blank(b) || is_break(b));
            if first.is_none()
                || first == Some(b'#')
                || marker
                || (!flow && next_column <= parent)
                || (flow && matches!(first, Some(b',' | b']' | b'}')))
            {
                break;
            }
            let saved = self.pos;
            self.pos = content;
            let (more, more_end) = self.plain_line(flow)?;
            if more_end == End::Key {
                self.pos = saved;
                return Err(Error::at(
                    "mapping values are not allowed in this context",
                    self.text,
                    content,
                ));
            }
            if blank_lines == 0 {
                text.push(' ');
            } else {
                text.push_str(&"\n".repeat(blank_lines));
            }
            text.push_str(&more);
            resume = self.pos;
            end = more_end;
            // a trailing comment or flow indicator on this line ends the scalar
            let rest = self.bytes.get(self.pos).copied();
            if rest.is_some_and(|b| !is_break(b)) {
                break;
            }
        }
        self.pos = resume;
        Ok((text, end))
    }

    /// One line of a plain scalar, trailing blanks trimmed.
    fn plain_line(&mut self, flow: bool) -> Result<(String, End)> {
        let start = self.pos;
        let mut end = End::Other;
        while let Some(byte) = self.peek() {
            if is_break(byte) {
                break;
            }
            if byte == b':'
                && self.peek_at(1).is_none_or(|b| {
                    is_blank(b) || is_break(b) || (flow && matches!(b, b',' | b']' | b'}'))
                })
            {
                end = End::Key;
                break;
            }
            if byte == b'#' && self.pos > start && is_blank(self.bytes[self.pos - 1]) {
                break;
            }
            if flow && matches!(byte, b',' | b'[' | b']' | b'{' | b'}') {
                break;
            }
            self.pos += 1;
        }
        let text = self.text[start..self.pos]
            .trim_end_matches([' ', '\t'])
            .to_string();
        if text.is_empty() && end == End::Other && self.pos == start {
            return Err(self.error("expected a value"));
        }
        Ok((text, end))
    }

    fn single_quoted(&mut self, _parent: isize) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated single-quoted string")),
                Some(b'\'') if self.peek_at(1) == Some(b'\'') => {
                    out.push('\'');
                    self.pos += 2;
                }
                Some(b'\'') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b) if is_break(b) => self.fold_quoted_break(&mut out)?,
                Some(_) => {
                    let ch = self.text[self.pos..].chars().next().unwrap_or('\u{fffd}');
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    /// A line break inside a quoted scalar: trailing blanks drop, the break
    /// becomes a space (or `\n` per blank line), leading blanks drop.
    fn fold_quoted_break(&mut self, out: &mut String) -> Result<()> {
        while out.ends_with([' ', '\t']) {
            out.pop();
        }
        let mut breaks = 0;
        while self.peek().is_some_and(|b| is_break(b) || is_blank(b)) {
            if self.peek().is_some_and(is_break) {
                self.skip_line_break();
                breaks += 1;
                if self.document_marker().is_some() {
                    return Err(self.error("a document marker inside a quoted string"));
                }
            } else {
                self.pos += 1;
            }
        }
        if breaks <= 1 {
            out.push(' ');
        } else {
            out.push_str(&"\n".repeat(breaks - 1));
        }
        Ok(())
    }

    fn double_quoted(&mut self, _parent: isize) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated double-quoted string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                Some(b) if is_break(b) => self.fold_quoted_break(&mut out)?,
                Some(_) => {
                    let ch = self.text[self.pos..].chars().next().unwrap_or('\u{fffd}');
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<()> {
        let Some(byte) = self.peek() else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        let simple = match byte {
            b'0' => '\0',
            b'a' => '\u{7}',
            b'b' => '\u{8}',
            b't' | b'\t' => '\t',
            b'n' => '\n',
            b'v' => '\u{b}',
            b'f' => '\u{c}',
            b'r' => '\r',
            b'e' => '\u{1b}',
            b' ' => ' ',
            b'"' => '"',
            b'/' => '/',
            b'\\' => '\\',
            b'N' => '\u{85}',
            b'_' => '\u{a0}',
            b'L' => '\u{2028}',
            b'P' => '\u{2029}',
            b'x' => return self.unicode(out, 2),
            b'u' => return self.unicode(out, 4),
            b'U' => return self.unicode(out, 8),
            b'\n' | b'\r' => {
                // an escaped line break joins the lines with nothing between
                self.pos -= 1;
                self.skip_line_break();
                self.skip_blanks();
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

    /// `|` literal and `>` folded scalars.
    fn block_scalar(&mut self, parent: isize) -> Result<Value> {
        let folded = self.bytes[self.pos] == b'>';
        self.pos += 1;
        let mut indent_indicator = None;
        let mut chomp = b'c';
        for _ in 0..2 {
            match self.peek() {
                Some(d @ b'1'..=b'9') if indent_indicator.is_none() => {
                    indent_indicator = Some(usize::from(d - b'0'));
                    self.pos += 1;
                }
                Some(c @ (b'+' | b'-')) if chomp == b'c' => {
                    chomp = c;
                    self.pos += 1;
                }
                _ => {}
            }
        }
        self.skip_blanks();
        if !self.at_line_end() {
            return Err(self.error("expected a line break after the block scalar header"));
        }
        // consume the rest of the header line (a comment may sit there)
        while self.peek().is_some_and(|b| !is_break(b)) {
            self.pos += 1;
        }
        self.skip_line_break();

        let base = usize::try_from(parent.max(0)).unwrap_or(0);
        let mut block_indent = indent_indicator.map(|n| base + n);
        let mut lines: Vec<String> = Vec::new();
        let mut last_terminated = true;
        loop {
            if self.pos >= self.bytes.len() {
                break;
            }
            let line_end = self.bytes[self.pos..]
                .iter()
                .position(|&b| is_break(b))
                .map_or(self.bytes.len(), |i| self.pos + i);
            let raw = &self.text[self.pos..line_end];
            let leading = raw.bytes().take_while(|&b| b == b' ').count();
            let blank = raw.trim_matches(' ').is_empty();
            if !blank && self.document_marker().is_some() {
                break;
            }
            if blank {
                // blank lines belong to the scalar; spaces beyond the indent are content
                lines.push(match block_indent {
                    Some(indent) if raw.len() > indent => raw[indent..].to_string(),
                    _ => String::new(),
                });
            } else {
                // the first non-blank line fixes the indentation
                let indent = *block_indent.get_or_insert(leading);
                if leading >= indent && isize::try_from(leading).unwrap_or(isize::MAX) > parent {
                    lines.push(raw[indent..].to_string());
                } else {
                    break;
                }
            }
            self.pos = line_end;
            last_terminated = self.peek().is_some_and(is_break);
            self.skip_line_break();
        }
        Ok(Value::String(assemble(
            &lines,
            folded,
            chomp,
            last_terminated,
        )))
    }

    fn flow(&mut self, parent: isize) -> Result<Value> {
        let value = self.flow_node(parent)?;
        Ok(value)
    }

    fn skip_flow_space(&mut self) -> Result<()> {
        loop {
            self.skip_blanks();
            match self.peek() {
                Some(b'#') if self.pos == 0 || self.bytes[self.pos - 1].is_ascii_whitespace() => {
                    while self.peek().is_some_and(|b| !is_break(b)) {
                        self.pos += 1;
                    }
                }
                Some(b) if is_break(b) => self.skip_line_break(),
                _ => return Ok(()),
            }
        }
    }

    fn flow_node(&mut self, parent: isize) -> Result<Value> {
        self.nested(|this| this.flow_node_inner(parent))
    }

    fn flow_node_inner(&mut self, parent: isize) -> Result<Value> {
        self.skip_flow_space()?;
        let mut anchor = None;
        let mut tag = None;
        loop {
            match self.peek() {
                Some(b'&') => anchor = Some(self.name()?),
                Some(b'!') => tag = Some(self.tag()),
                _ => break,
            }
            self.skip_flow_space()?;
        }
        let mut raw = None;
        let value = match self.peek() {
            None => return Err(self.error("unterminated flow collection")),
            Some(b'[') => self.flow_sequence(parent)?,
            Some(b'{') => self.flow_mapping(parent)?,
            Some(b'"') => Value::String(self.double_quoted(parent)?),
            Some(b'\'') => Value::String(self.single_quoted(parent)?),
            Some(b'*') => {
                let name = self.name()?;
                self.anchors
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| self.error(format!("unknown anchor `{name}`")))?
            }
            Some(b',' | b']' | b'}') => Value::Null,
            Some(b':')
                if self.peek_at(1).is_none_or(|b| {
                    is_blank(b) || is_break(b) || matches!(b, b',' | b']' | b'}')
                }) =>
            {
                return Err(self.error("a mapping entry needs a key before `:`"));
            }
            Some(_) => {
                let (text, _) = self.plain_in(parent, false, true)?;
                let value = resolve::plain(&text);
                raw = Some(text);
                value
            }
        };
        let value = apply_tag(tag.as_deref(), value, raw.as_deref());
        if let Some(name) = anchor {
            self.anchors.insert(name, value.clone());
        }
        Ok(value)
    }

    fn flow_sequence(&mut self, parent: isize) -> Result<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_flow_space()?;
            match self.peek() {
                None => return Err(self.error("unterminated flow sequence")),
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                Some(b',') => return Err(self.error("unexpected `,` in the flow sequence")),
                _ => {}
            }
            let start_text = self.pos;
            let item = self.flow_node(parent)?;
            self.skip_flow_space()?;
            // `[a: b]` is a one-pair mapping
            let item = if self.peek() == Some(b':') {
                self.pos += 1;
                let key = scalar_key(&item, &self.text[start_text..self.pos - 1]);
                let value = self.flow_pair_value(parent)?;
                let mut map = Map::new();
                map.insert(key, value);
                Value::Object(map)
            } else {
                item
            };
            items.push(item);
            self.skip_flow_space()?;
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {}
                _ => return Err(self.error("expected `,` or `]` in the flow sequence")),
            }
        }
    }

    fn flow_pair_value(&mut self, parent: isize) -> Result<Value> {
        self.skip_flow_space()?;
        if matches!(self.peek(), Some(b',' | b'}' | b']')) {
            Ok(Value::Null)
        } else {
            self.flow_node(parent)
        }
    }

    fn flow_mapping(&mut self, parent: isize) -> Result<Value> {
        self.pos += 1;
        let mut map = Map::new();
        loop {
            self.skip_flow_space()?;
            match self.peek() {
                None => return Err(self.error("unterminated flow mapping")),
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                Some(b',') => return Err(self.error("unexpected `,` in the flow mapping")),
                _ => {}
            }
            let start_text = self.pos;
            let key_node = self.flow_key(parent)?;
            let key = scalar_key(&key_node, self.text[start_text..self.pos].trim_end());
            self.skip_flow_space()?;
            let value = if self.peek() == Some(b':') {
                self.pos += 1;
                self.flow_pair_value(parent)?
            } else {
                Value::Null
            };
            if map.contains_key(&key) {
                return Err(self.error(format!("duplicate mapping key `{key}`")));
            }
            map.insert(key, value);
            self.skip_flow_space()?;
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(self.error("expected `,` or `}` in the flow mapping")),
            }
        }
    }

    /// A key inside `{ }`: it ends at `:` (with or without a following blank
    /// when it was quoted), `,` or `}`.
    fn flow_key(&mut self, parent: isize) -> Result<Value> {
        self.flow_node(parent)
    }
}

/// A mapping key as text: strings as they are, other scalars as written.
fn scalar_key(node: &Value, written: &str) -> String {
    match node {
        Value::String(s) => s.clone(),
        Value::Null if written.is_empty() => String::new(),
        _ => written.trim().to_string(),
    }
}

/// Applies the tags that change a scalar's type; unknown tags are ignored.
fn apply_tag(tag: Option<&str>, value: Value, plain_text: Option<&str>) -> Value {
    match (tag, value) {
        (
            Some("!!str" | "tag:yaml.org,2002:str"),
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::UInt(_) | Value::Float(_),
        ) => Value::String(plain_text.unwrap_or_default().to_string()),
        (Some("!!null"), _) => Value::Null,
        // Any other tag keeps a plain scalar as the text that was written.
        (
            Some(other),
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::UInt(_) | Value::Float(_),
        ) if plain_text.is_some()
            && other.starts_with("!!")
            && !matches!(other, "!!int" | "!!float" | "!!bool") =>
        {
            Value::String(plain_text.unwrap_or_default().to_string())
        }
        (Some("!!int" | "!!float" | "!!bool"), Value::String(text)) => resolve::plain(&text),
        #[allow(clippy::cast_precision_loss)]
        (Some("!!float"), Value::UInt(n)) => Value::from_f64(n as f64),
        (_, value) => value,
    }
}

/// Joins the collected block-scalar lines per the style and chomping rules.
fn assemble(lines: &[String], folded: bool, chomp: u8, last_terminated: bool) -> String {
    // Trailing blank lines are the "tail"; chomping decides what survives.
    let content_end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    let (content, tail) = lines.split_at(content_end);
    let mut text = if folded {
        fold(content)
    } else {
        content.join("\n")
    };
    match chomp {
        b'-' => {}
        b'+' => {
            // The last line only has a break to keep if the input had one.
            if !content.is_empty() && (last_terminated || !tail.is_empty()) {
                text.push('\n');
            }
            text.push_str(&"\n".repeat(tail.len().saturating_sub(usize::from(!last_terminated))));
        }
        _ => {
            if !content.is_empty() && (last_terminated || !tail.is_empty()) {
                text.push('\n');
            }
        }
    }
    text
}

fn fold(lines: &[String]) -> String {
    let more_indented = |l: &str| l.starts_with([' ', '\t']);
    let mut out = String::new();
    let mut previous: Option<&str> = None;
    let mut blanks = 0;
    for line in lines {
        if line.is_empty() {
            blanks += 1;
            continue;
        }
        match previous {
            Some(p) if more_indented(p) || more_indented(line) => {
                out.push('\n');
                out.push_str(&"\n".repeat(blanks));
            }
            Some(_) if blanks == 0 => out.push(' '),
            Some(_) | None => out.push_str(&"\n".repeat(blanks)),
        }
        out.push_str(line);
        previous = Some(line);
        blanks = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn y(text: &str) -> Value {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    fn j(text: &str) -> Value {
        json::parse(text).unwrap()
    }

    fn bad(text: &str) -> String {
        parse(text).expect_err(text).to_string()
    }

    #[quench_test::test]
    fn scalars_and_types() {
        assert_eq!(
            y("a: 1\nb: -2\nc: 1.5\nd: true\ne: null\nf: ~\ng: text\nh: 'quoted'\ni: \"dq\"\nj: 0x1f\nk: .inf\nl:\n"),
            j(r#"{"a":1,"b":-2,"c":1.5,"d":true,"e":null,"f":null,"g":"text","h":"quoted","i":"dq","j":31,"k":null,"l":null}"#)
                .as_object()
                .map(|m| {
                    let mut m = m.clone();
                    m.insert("k", Value::Float(f64::INFINITY));
                    Value::Object(m)
                })
                .unwrap()
        );
        assert_eq!(y("yes: no\n"), j(r#"{"yes":"no"}"#));
    }

    #[quench_test::test]
    fn a_bare_scalar_document() {
        assert_eq!(y("hello"), Value::from("hello"));
        assert_eq!(y("42"), Value::from(42));
        assert_eq!(y(""), Value::Null);
        assert_eq!(y("# only a comment\n"), Value::Null);
        assert_eq!(y("~"), Value::Null);
        assert_eq!(y("\"x\""), Value::from("x"));
    }

    #[quench_test::test]
    fn block_mappings_and_sequences() {
        assert_eq!(
            y("a:\n  b:\n    c: 1\n  d: 2\ne: 3\n"),
            j(r#"{"a":{"b":{"c":1},"d":2},"e":3}"#)
        );
        assert_eq!(
            y("- a\n- b\n- - c\n  - d\n- e: 1\n  f: 2\n"),
            j(r#"["a","b",["c","d"],{"e":1,"f":2}]"#)
        );
        assert_eq!(y("a:\n- 1\n- 2\nb: x\n"), j(r#"{"a":[1,2],"b":"x"}"#));
        assert_eq!(y("a:\n  - 1\n  - 2\nb: x\n"), j(r#"{"a":[1,2],"b":"x"}"#));
        assert_eq!(
            y("- name: n1\n  items:\n  - a\n  - b\n- name: n2\n  items: []\n"),
            j(r#"[{"name":"n1","items":["a","b"]},{"name":"n2","items":[]}]"#)
        );
        assert_eq!(
            y("a:\n  - b\n  -\n    c: d\n"),
            j(r#"{"a":["b",{"c":"d"}]}"#)
        );
        assert_eq!(y("- \n- x\n-\n"), j(r#"[null,"x",null]"#));
        assert_eq!(y("empty:\nnext: 1\n"), j(r#"{"empty":null,"next":1}"#));
    }

    #[quench_test::test]
    fn keys() {
        assert_eq!(
            y("key with spaces: v\n\"quoted key\": v2\n'single key': v3\n1: one\ntrue: t\n~: n\n"),
            j(
                r#"{"key with spaces":"v","quoted key":"v2","single key":"v3","1":"one","true":"t","~":"n"}"#
            )
        );
        assert_eq!(y("a:b: c\n"), j(r#"{"a:b":"c"}"#));
        assert!(bad("a: 1\na: 2\n").contains("duplicate mapping key"));
        assert!(bad("{a: 1, a: 2}").contains("duplicate"));
    }

    #[quench_test::test]
    fn comments() {
        assert_eq!(
            y("# c\na: 1 # trailing\n# between\nb: 2 #x\nc: 'q # not a comment'\nd: a#b\n"),
            j(r#"{"a":1,"b":2,"c":"q # not a comment","d":"a#b"}"#)
        );
        assert_eq!(y("a: # why\n  b: 1\n"), j(r#"{"a":{"b":1}}"#));
        assert_eq!(y("- # first\n  x\n"), j(r#"["x"]"#));
    }

    #[quench_test::test]
    fn plain_scalars_fold_across_lines() {
        assert_eq!(y("- a\n- b\n  - c\n"), j(r#"["a","b - c"]"#));
        assert_eq!(
            y("a: plain\n  continued\n  more\nb: 2\n"),
            j(r#"{"a":"plain continued more","b":2}"#)
        );
        assert_eq!(y("a: one\n\n  two\n"), j(r#"{"a":"one\ntwo"}"#));
        assert_eq!(y("- a\n  b\n- c\n"), j(r#"["a b","c"]"#));
        assert_eq!(y("a\n\t\tb\n"), Value::from("a b"));
        assert_eq!(
            y("a: x\n # comment line\nb: y\n"),
            j(r#"{"a":"x","b":"y"}"#)
        );
        assert_eq!(
            y("url: http://x.y/z?q=1#frag\nt: a:b\nu: a #c\n"),
            j(r#"{"url":"http://x.y/z?q=1#frag","t":"a:b","u":"a"}"#)
        );
        assert!(bad("a: x\n  b: y\n").contains("mapping values are not allowed"));
    }

    #[quench_test::test]
    fn quoted_scalars() {
        assert_eq!(
            y(r#"a: "q \n \t \" \\ \/ \u00e9 \x41 \U0001F600 \0 \a \b \e \f \r \v \N \_ \L \P \  end""#)["a"].as_str(),
            Some("q \n \t \" \\ / é A 😀 \0 \u{7} \u{8} \u{1b} \u{c} \r \u{b} \u{85} \u{a0} \u{2028} \u{2029}   end")
        );
        assert_eq!(y(r#"a: "tab\there\n""#), j(r#"{"a":"tab\there\n"}"#));
        assert_eq!(
            y("a: 'it''s'\nb: ''\nc: \"\"\n"),
            j(r#"{"a":"it's","b":"","c":""}"#)
        );
        assert_eq!(
            y("a: \"multi\n  line\n\n  para\"\n"),
            j(r#"{"a":"multi line\npara"}"#)
        );
        assert_eq!(y("a: 'multi\n  line'\n"), j(r#"{"a":"multi line"}"#));
        assert_eq!(y("a: \"esc \\\n   joined\"\n"), j(r#"{"a":"esc joined"}"#));
        assert_eq!(y("a: \"trail  \n  fold\"\n"), j(r#"{"a":"trail fold"}"#));
        for text in [
            "a: \"unterminated",
            "a: 'unterminated",
            "a: \"bad \\q\"",
            "a: \"\\x4\"",
            "a: \"\\uZZZZ\"",
            "a: \"\\U00110000\"",
            "a: \"\\",
            "a: \"x\n---\ny\"",
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }

    #[quench_test::test]
    fn block_scalars() {
        assert_eq!(
            y("s: |\n  line1\n  line2\nt: x\n"),
            j(r#"{"s":"line1\nline2\n","t":"x"}"#)
        );
        assert_eq!(y("s: |-\n  strip\n"), j(r#"{"s":"strip"}"#));
        assert_eq!(
            y("s: |+\n  keep\n\nt: x\n"),
            j(r#"{"s":"keep\n\n","t":"x"}"#)
        );
        assert_eq!(
            y("s: >\n  folded\n  text\n\n  para\n"),
            j(r#"{"s":"folded text\npara\n"}"#)
        );
        assert_eq!(y("s: >-\n  a\n   b\n  c\n"), j(r#"{"s":"a\n b\nc"}"#));
        assert_eq!(
            y("s: |2\n    indented\n  base\n"),
            j(r#"{"s":"  indented\nbase\n"}"#)
        );
        assert_eq!(
            y("s: |\n\n  after blank\n"),
            j(r#"{"s":"\nafter blank\n"}"#)
        );
        assert_eq!(y("s: |  # comment\n  x\n"), j(r#"{"s":"x\n"}"#));
        assert_eq!(y("s: |\n  x\n\n\nt: 1\n"), j(r#"{"s":"x\n","t":1}"#));
        assert_eq!(
            y("s: |\n  a\n   more\n  b\n"),
            j(r#"{"s":"a\n more\nb\n"}"#)
        );
        assert_eq!(y("s: |\n"), j(r#"{"s":""}"#));
        assert_eq!(y("s: |+\n\n"), j(r#"{"s":"\n"}"#));
        assert_eq!(y("- |\n text\n- >\n more\n"), j(r#"["text\n","more\n"]"#));
        assert_eq!(y("--- |\n  top\n"), Value::from("top\n"));
        assert_eq!(
            y("s: |\r\n  crlf\r\n  lines\r\n")["s"]
                .as_str()
                .map(|t| t.replace('\r', "")),
            Some("crlf\nlines\n".to_string())
        );
        // a final line without its own break keeps none
        assert_eq!(y("s: |\n  last")["s"], Value::from("last"));
        assert_eq!(y("s: |+\n  last")["s"], Value::from("last"));
        assert!(
            bad("s: |x\n  y\n").contains("expected a line break after the block scalar header")
        );
    }

    #[quench_test::test]
    fn flow_collections() {
        assert_eq!(
            y("{a: 1, b: [x, y, {c: d}], e: ~, f}"),
            j(r#"{"a":1,"b":["x","y",{"c":"d"}],"e":null,"f":null}"#)
        );
        assert_eq!(
            y("[a, b: c, {d: e}, 'f', \"g\", ]"),
            j(r#"["a",{"b":"c"},{"d":"e"},"f","g"]"#)
        );
        assert_eq!(
            y("a: [1,\n  2,\n  3]\nb: {x: 1,\n  y: 2}\n"),
            j(r#"{"a":[1,2,3],"b":{"x":1,"y":2}}"#)
        );
        assert_eq!(y("[]"), j("[]"));
        assert_eq!(y("{}"), j("{}"));
        assert_eq!(
            y("{\"a\":1,\"b\":[true,null]}"),
            j(r#"{"a":1,"b":[true,null]}"#)
        );
        assert_eq!(y("[ # c\n  1, # d\n  2\n]"), j("[1,2]"));
        assert_eq!(y("[a b, c\n d]"), j(r#"["a b","c d"]"#));
        assert_eq!(y("{a: , b: 2}"), j(r#"{"a":null,"b":2}"#));
        assert_eq!(
            y("{a: [1, 2], b: {c: d}}"),
            j(r#"{"a":[1,2],"b":{"c":"d"}}"#)
        );
        assert_eq!(y("k: [a,b]\n"), j(r#"{"k":["a","b"]}"#));
        for text in [
            "[1, 2",
            "{a: 1",
            "[1,, 2]",
            "{a: 1,, b: 2}",
            "[,]",
            "{,}",
            "{a: 1, a: 2}",
            "[a: b",
            "a: [1 2",
        ] {
            // "[1 2]" is one plain scalar "1 2", so only genuine breakage is listed here
            if text != "a: [1 2" {
                assert!(parse(text).is_err(), "{text:?}");
            }
        }
    }

    #[quench_test::test]
    fn anchors_aliases_and_tags() {
        let v =
            y("base: &b {x: 1}\ncopy: *b\nlist: &l [1, 2]\nagain: *l\nscalar: &s text\nuse: *s\n");
        assert_eq!(v["copy"], j(r#"{"x":1}"#));
        assert_eq!(v["again"], j("[1,2]"));
        assert_eq!(v["use"], Value::from("text"));
        assert_eq!(
            y("- &a x\n- *a\n- &b\n  k: v\n- *b\n"),
            j(r#"["x","x",{"k":"v"},{"k":"v"}]"#)
        );
        // properties on the same line as a key label the key
        assert_eq!(y("- &b k: v\n- *b\n"), j(r#"[{"k":"v"},"k"]"#));
        assert_eq!(
            y("a: &x\n  b: 1\nc: *x\n"),
            j(r#"{"a":{"b":1},"c":{"b":1}}"#)
        );
        assert_eq!(y("[&a 1, *a]"), j("[1,1]"));
        assert!(bad("a: *missing\n").contains("unknown anchor"));
        assert!(bad("[*missing]").contains("unknown anchor"));
        assert!(bad("a: &\n").contains("needs a name"));
        assert!(bad("- &a x\n- *a : 1\n").contains("alias cannot be a mapping key"));

        assert_eq!(
            y(
                "a: !!str 123\nb: !!int \"45\"\nc: !custom thing\nd: !!float 3\ne: !!null x\nf: !!bool \"true\"\ng: !!str true\n"
            ),
            j(r#"{"a":"123","b":45,"c":"thing","d":3.0,"e":null,"f":true,"g":"true"}"#)
        );
        assert_eq!(y("a: !!weird 3\nb: !local 3\n"), j(r#"{"a":"3","b":3}"#));
        assert_eq!(y("a: !tagged\n  b: 1\n"), j(r#"{"a":{"b":1}}"#));
        assert_eq!(y("a: !t\n- x\n"), j(r#"{"a":["x"]}"#));
        assert_eq!(y("!!str\n"), Value::from(""));
    }

    #[quench_test::test]
    fn documents() {
        assert_eq!(
            parse_all("---\na: 1\n...\n---\nb: 2\n").unwrap(),
            vec![j(r#"{"a":1}"#), j(r#"{"b":2}"#)]
        );
        assert_eq!(parse_all("a: 1\n---\nb: 2\n").unwrap().len(), 2);
        assert_eq!(
            parse_all("--- [1, 2]\n--- {a: 1}\n--- text\n").unwrap(),
            vec![j("[1,2]"), j(r#"{"a":1}"#), Value::from("text")]
        );
        assert_eq!(parse_all("").unwrap(), Vec::<Value>::new());
        assert_eq!(
            parse_all("---\n---\n").unwrap(),
            vec![Value::Null, Value::Null]
        );
        assert_eq!(
            parse_all("%YAML 1.2\n---\na: 1\n").unwrap(),
            vec![j(r#"{"a":1}"#)]
        );
        assert_eq!(parse_all("a: 1\n...\n").unwrap().len(), 1);
        assert_eq!(parse_all("...\n").unwrap().len(), 0);
        assert_eq!(parse_all("\u{feff}a: 1\n").unwrap(), vec![j(r#"{"a":1}"#)]);
        // anchors do not leak across documents
        assert!(parse_all("a: &x 1\n---\nb: *x\n").is_err());
        assert!(bad("a: 1\n---\nb: 2\n").contains("single YAML document"));
        assert!(
            bad("a: 1\n... x\nb: 2").contains("unexpected content")
                || parse("a: 1\n... x\nb: 2").is_err()
        );
    }

    #[quench_test::test]
    fn line_endings_and_whitespace() {
        assert_eq!(
            y("a: 1\r\nb:\r\n  - x\r\n  - y\r\n"),
            j(r#"{"a":1,"b":["x","y"]}"#)
        );
        assert_eq!(y("a: 1\n\n\n\nb: 2\n"), j(r#"{"a":1,"b":2}"#));
        assert_eq!(y("a:\t1\n"), j(r#"{"a":1}"#));
        assert_eq!(y("a: 1   \nb: 2\t\n"), j(r#"{"a":1,"b":2}"#));
        assert!(bad("a:\n\t- b\n").contains("tabs cannot be used for indentation"));
    }

    #[quench_test::test]
    fn structural_errors_have_positions() {
        for (text, needle) in [
            ("a: b: c\n", "mapping values are not allowed"),
            ("a: - b\n", "block sequence entries are not allowed"),
            ("- a\n b: c\n", "mapping values are not allowed"),
            ("  a: 1\n b: 2\n", "unexpected content"),
            ("a: 1\n  b: 2\n", "mapping values are not allowed"),
            ("? complex\n: v\n", "complex mapping keys"),
            ("a:\n  ? k\n", "complex mapping keys"),
            ("a: 1\n? k\n", "complex mapping keys"),
            ("a: 1\n- b\n", "sequence entry is not allowed"),
            ("a: @x\n", "cannot start a value"),
            ("a: `x\n", "cannot start a value"),
            ("a: ,x\n", "cannot start a value"),
            ("a: ]x\n", "cannot start a value"),
            (": v\n", "needs a key"),
            ("a: 1\n: v\n", "needs a key"),
            ("a: 1\nfoo\n", "could not find the expected"),
            ("a: [x]: 1\n", "collection cannot be a mapping key"),
        ] {
            let e = parse(text).expect_err(text);
            assert!(e.to_string().contains(needle), "{text:?}: {e}");
        }
        assert_eq!(
            parse("a: 1\nb: [\n").unwrap_err().location().map(|l| l.0),
            Some(3)
        );
    }

    #[quench_test::test]
    fn non_string_keys_are_read_as_their_text() {
        assert_eq!(
            y("1: a\n2.5: b\ntrue: c\nnull: d\n"),
            j(r#"{"1":"a","2.5":"b","true":"c","null":"d"}"#)
        );
        assert_eq!(y("{1: a, true: b}"), j(r#"{"1":"a","true":"b"}"#));
    }
}
