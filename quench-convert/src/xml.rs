//! XML text, as a tree of plain values.
//!
//! An element is an object with three keys: `"tag"` (the name as written, prefix included), `"attrs"` (an
//! object of attribute values, in document order) and `"children"` (an array of elements and strings). A
//! document is its root element. Comments, processing instructions, the XML declaration and the doctype
//! are skipped; CDATA becomes text; the five predefined entities and numeric references are decoded.
//! Text that is only whitespace between elements is dropped unless [`Options::keep_whitespace`] is set.
//!
//! ```
//! use quench_convert::xml;
//!
//! let doc = xml::parse(r#"<list n="2"><item>a</item><item>b &amp; c</item></list>"#).unwrap();
//! assert_eq!(xml::tag(&doc), Some("list"));
//! assert_eq!(xml::attr(&doc, "n"), Some("2"));
//! let items: Vec<_> = xml::elements(&doc, "item").map(xml::text).collect();
//! assert_eq!(items, ["a", "b & c"]);
//! assert_eq!(xml::to_string(&doc), r#"<list n="2"><item>a</item><item>b &amp; c</item></list>"#);
//! ```

use crate::error::{Error, Result};
use quench_serde::{Map, Value};

/// How to read XML.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Keep text nodes that are only whitespace.
    pub keep_whitespace: bool,
}

/// An element value.
#[must_use]
pub fn element(tag: &str, attrs: Map, children: Vec<Value>) -> Value {
    let mut map = Map::new();
    map.insert("tag", Value::String(tag.to_string()));
    map.insert("attrs", Value::Object(attrs));
    map.insert("children", Value::Array(children));
    Value::Object(map)
}

/// The tag of an element value.
#[must_use]
pub fn tag(element: &Value) -> Option<&str> {
    element.get("tag")?.as_str()
}

/// An attribute of an element value.
#[must_use]
pub fn attr<'a>(element: &'a Value, name: &str) -> Option<&'a str> {
    element.get("attrs")?.get(name)?.as_str()
}

/// The children of an element value (elements and strings).
#[must_use]
pub fn children(element: &Value) -> &[Value] {
    element
        .get("children")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The child elements with the given tag.
pub fn elements<'a>(element: &'a Value, name: &'a str) -> impl Iterator<Item = &'a Value> {
    children(element)
        .iter()
        .filter(move |c| tag(c) == Some(name))
}

/// The first child element with the given tag.
#[must_use]
pub fn child<'a>(element: &'a Value, name: &str) -> Option<&'a Value> {
    children(element).iter().find(|c| tag(c) == Some(name))
}

/// All the text inside an element, descendants included, concatenated.
#[must_use]
pub fn text(element: &Value) -> String {
    let mut out = String::new();
    for c in children(element) {
        match c {
            Value::String(s) => out.push_str(s),
            other => out.push_str(&text(other)),
        }
    }
    out
}

struct Reader<'a> {
    src: &'a str,
    pos: usize,
    options: &'a Options,
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

impl Reader<'_> {
    fn line(&self) -> usize {
        1 + self.src[..self.pos].bytes().filter(|&b| b == b'\n').count()
    }

    fn error(&self, message: &str) -> Error {
        Error::new("xml", message, self.line())
    }

    fn rest(&self) -> &str {
        &self.src[self.pos..]
    }

    fn skip_space(&mut self) {
        let trimmed = self.rest().trim_start();
        self.pos = self.src.len() - trimmed.len();
    }

    fn skip_to(&mut self, end: &str, what: &str) -> Result<()> {
        match self.rest().find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => Err(self.error(&format!("{what} is never closed"))),
        }
    }

    fn name(&mut self) -> Result<String> {
        let len = self
            .rest()
            .find(|c: char| !is_name_char(c))
            .unwrap_or(self.rest().len());
        if len == 0 {
            return Err(self.error("expected a name"));
        }
        let name = self.rest()[..len].to_string();
        self.pos += len;
        Ok(name)
    }

    /// Skips what can sit between nodes but is not part of the tree.
    fn skip_misc(&mut self) -> Result<bool> {
        let rest = self.rest();
        if rest.starts_with("<!--") {
            self.skip_to("-->", "a comment")?;
        } else if rest.starts_with("<?") {
            self.skip_to("?>", "a processing instruction")?;
        } else if rest.starts_with("<!DOCTYPE") || rest.starts_with("<!doctype") {
            // A doctype may hold an internal subset in brackets.
            let mut depth = 0usize;
            loop {
                let Some(c) = self.rest().chars().next() else {
                    return Err(self.error("the doctype is never closed"));
                };
                self.pos += c.len_utf8();
                match c {
                    '[' => depth += 1,
                    ']' => depth = depth.saturating_sub(1),
                    '>' if depth == 0 => break,
                    _ => {}
                }
            }
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    fn decode(&self, raw: &str) -> Result<String> {
        if !raw.contains('&') {
            return Ok(raw.to_string());
        }
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(i) = rest.find('&') {
            out.push_str(&rest[..i]);
            rest = &rest[i + 1..];
            let end = rest
                .find(';')
                .ok_or_else(|| self.error("an entity is missing its `;`"))?;
            let entity = &rest[..end];
            rest = &rest[end + 1..];
            match entity {
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "amp" => out.push('&'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    let code = entity
                        .strip_prefix("#x")
                        .map(|h| u32::from_str_radix(h, 16))
                        .or_else(|| entity.strip_prefix('#').map(str::parse::<u32>));
                    let c = code
                        .and_then(std::result::Result::ok)
                        .and_then(char::from_u32)
                        .ok_or_else(|| self.error(&format!("unknown entity `&{entity};`")))?;
                    out.push(c);
                }
            }
        }
        out.push_str(rest);
        Ok(out)
    }

    fn element(&mut self, depth: usize) -> Result<Value> {
        if depth > 256 {
            return Err(self.error("elements nest too deeply"));
        }
        self.pos += 1; // '<'
        let tag = self.name()?;
        let mut attrs = Map::new();
        loop {
            self.skip_space();
            let rest = self.rest();
            if rest.starts_with("/>") {
                self.pos += 2;
                return Ok(element(&tag, attrs, Vec::new()));
            }
            if rest.starts_with('>') {
                self.pos += 1;
                break;
            }
            let name = self.name()?;
            self.skip_space();
            if !self.rest().starts_with('=') {
                return Err(self.error(&format!("attribute `{name}` has no value")));
            }
            self.pos += 1;
            self.skip_space();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\''));
            let Some(quote) = quote else {
                return Err(self.error(&format!("attribute `{name}` is not quoted")));
            };
            self.pos += 1;
            let end = self
                .rest()
                .find(quote)
                .ok_or_else(|| self.error("an attribute value is never closed"))?;
            let value = self.decode(&self.rest()[..end])?;
            self.pos += end + 1;
            attrs.insert(name, Value::String(value));
        }

        let mut children: Vec<Value> = Vec::new();
        let mut pending = String::new();
        let flush = |pending: &mut String, children: &mut Vec<Value>, keep: bool| {
            if !pending.is_empty() && (keep || !pending.trim().is_empty()) {
                children.push(Value::String(std::mem::take(pending)));
            } else {
                pending.clear();
            }
        };
        loop {
            let Some(i) = self.rest().find('<') else {
                return Err(self.error(&format!("element `{tag}` is never closed")));
            };
            let raw = &self.rest()[..i];
            let decoded = self.decode(raw)?;
            pending.push_str(&decoded);
            self.pos += i;
            let rest = self.rest();
            if rest.starts_with("</") {
                self.pos += 2;
                let close = self.name()?;
                self.skip_space();
                if !self.rest().starts_with('>') {
                    return Err(self.error("expected `>`"));
                }
                self.pos += 1;
                if close != tag {
                    return Err(self.error(&format!("`</{close}>` closes `<{tag}>`")));
                }
                flush(&mut pending, &mut children, self.options.keep_whitespace);
                return Ok(element(&tag, attrs, children));
            } else if rest.starts_with("<![CDATA[") {
                self.pos += 9;
                let end = self
                    .rest()
                    .find("]]>")
                    .ok_or_else(|| self.error("CDATA is never closed"))?;
                pending.push_str(&self.rest()[..end]);
                self.pos += end + 3;
            } else if self.skip_misc()? {
            } else {
                flush(&mut pending, &mut children, self.options.keep_whitespace);
                children.push(self.element(depth + 1)?);
            }
        }
    }
}

/// Reads an XML document into its root element.
///
/// # Errors
///
/// Malformed markup: mismatched or unclosed tags, unquoted attributes, unknown entities, no root element.
pub fn parse(text: &str) -> Result<Value> {
    parse_with(text, &Options::default())
}

/// [`parse`] with options.
///
/// # Errors
///
/// As [`parse`].
pub fn parse_with(text: &str, options: &Options) -> Result<Value> {
    let src = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut reader = Reader {
        src,
        pos: 0,
        options,
    };
    loop {
        reader.skip_space();
        if !reader.skip_misc()? {
            break;
        }
    }
    if !reader.rest().starts_with('<') {
        return Err(reader.error("expected a root element"));
    }
    let root = reader.element(0)?;
    loop {
        reader.skip_space();
        if reader.rest().is_empty() {
            return Ok(root);
        }
        if !reader.skip_misc()? {
            return Err(reader.error("content after the root element"));
        }
    }
}

fn escape(text: &str, attribute: bool, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

fn write(value: &Value, indent: Option<usize>, level: usize, out: &mut String) {
    let pad = |out: &mut String, level: usize| {
        if let Some(width) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(width * level));
        }
    };
    match value {
        Value::String(s) => escape(s, false, out),
        element => {
            let name = tag(element).unwrap_or("element");
            out.push('<');
            out.push_str(name);
            if let Some(attrs) = element.get("attrs").and_then(Value::as_object) {
                for (k, v) in attrs.iter() {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    escape(v.as_str().unwrap_or_default(), true, out);
                    out.push('"');
                }
            }
            let kids = children(element);
            if kids.is_empty() {
                out.push_str("/>");
                return;
            }
            out.push('>');
            let only_text = kids.iter().all(|k| matches!(k, Value::String(_)));
            for kid in kids {
                if !only_text {
                    pad(out, level + 1);
                }
                write(kid, indent, level + 1, out);
            }
            if !only_text {
                pad(out, level);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    }
}

/// Writes an element value as XML text with no added whitespace. A childless element is written
/// self-closing.
#[must_use]
pub fn to_string(value: &Value) -> String {
    let mut out = String::new();
    write(value, None, 0, &mut out);
    out
}

/// [`to_string`], with elements that hold elements on their own lines, indented by `width` spaces.
#[must_use]
pub fn to_string_pretty(value: &Value, width: usize) -> String {
    let mut out = String::new();
    write(value, Some(width), 0, &mut out);
    out
}
