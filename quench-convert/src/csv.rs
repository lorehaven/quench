//! Delimited text (RFC 4180): quoted fields, doubled quotes, embedded newlines, CRLF or LF rows.
//!
//! ```
//! use quench_convert::csv::{self, Options};
//!
//! let rows = csv::parse("name,qty\nbolt,4\n\"nut, hex\",9\n", &Options::default()).unwrap();
//! assert_eq!(rows[1]["name"], "nut, hex");
//! assert_eq!(csv::to_string(&rows, b','), "name,qty\nbolt,4\n\"nut, hex\",9\n");
//! ```

use crate::error::{Error, Result};
use quench_serde::{Map, Value};

/// How to read delimited text.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    pub delimiter: u8,
    /// Whether the first record names the columns. Records then become objects keyed by those names (a
    /// field past the last name is keyed `column<N>`, counting from 1); otherwise arrays of strings.
    pub header: bool,
    /// Whether records may have different numbers of fields. When not, a record that differs from the
    /// first is an error.
    pub flexible: bool,
    /// Whether to trim spaces around every unquoted field (and header name).
    pub trim: bool,
    /// Whether to refuse malformed quoting (an unclosed quote, text after a closing quote) instead of
    /// reading it as most programs do.
    pub strict: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            delimiter: b',',
            header: true,
            flexible: true,
            trim: false,
            strict: false,
        }
    }
}

/// The raw records of `text`, with no header handling.
///
/// Lenient like most readers: a quote inside an
/// unquoted field is just a character, text after a closing quote joins the field, and a quote left open
/// at the end of the text runs to the end.
///
/// # Errors
///
/// Never, in this lenient mode; see [`Options::strict`] and [`parse`] for the strict one.
pub fn records(text: &str, delimiter: u8, trim: bool) -> Result<Vec<Vec<String>>> {
    scan(text, delimiter, trim, false)
}

fn scan(text: &str, delimiter: u8, trim: bool, strict: bool) -> Result<Vec<Vec<String>>> {
    let delimiter = char::from(delimiter);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    // Whether a field is in progress: it has text, or a delimiter just announced it.
    let mut open = false;
    // Whether the field was quoted, which keeps its spaces.
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut line = 1;
    let finish = |field: &mut String, record: &mut Vec<String>, quoted: &mut bool| {
        let value = std::mem::take(field);
        record.push(if trim && !*quoted {
            value.trim().to_string()
        } else {
            value
        });
        *quoted = false;
    };

    while let Some(c) = chars.next() {
        match c {
            '"' if !quoted && (field.is_empty() || (trim && field.trim().is_empty())) => {
                field.clear();
                let opened = line;
                loop {
                    match chars.next() {
                        None if strict => {
                            return Err(Error::new(
                                "csv",
                                "a quoted field is never closed",
                                opened,
                            ));
                        }
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            field.push('"');
                        }
                        None | Some('"') => break,
                        Some(ch) => {
                            if ch == '\n' {
                                line += 1;
                            }
                            field.push(ch);
                        }
                    }
                }
                quoted = true;
                open = true;
                if strict
                    && !matches!(chars.peek(), None | Some('\n' | '\r'))
                    && chars.peek() != Some(&delimiter)
                {
                    return Err(Error::new("csv", "text after a closing quote", line));
                }
            }
            c if c == delimiter => {
                finish(&mut field, &mut record, &mut quoted);
                open = true;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                if open {
                    finish(&mut field, &mut record, &mut quoted);
                    open = false;
                }
                if !record.is_empty() {
                    out.push(std::mem::take(&mut record));
                }
            }
            c => {
                field.push(c);
                open = true;
            }
        }
    }
    if open {
        finish(&mut field, &mut record, &mut quoted);
    }
    if !record.is_empty() {
        out.push(record);
    }
    Ok(out)
}

/// Reads delimited text into values: objects (with a header row) or arrays of strings.
///
/// # Errors
///
/// Malformed quoting when `strict` is on, and a record with a different field count when `flexible` is off.
pub fn parse(text: &str, options: &Options) -> Result<Vec<Value>> {
    let rows = scan(text, options.delimiter, options.trim, options.strict)?;
    let width = rows.first().map(Vec::len);
    if !options.flexible
        && let Some(i) = rows.iter().position(|r| Some(r.len()) != width)
    {
        return Err(Error::new(
            "csv",
            format!("record {} has a different number of fields", i + 1),
            i + 1,
        ));
    }
    let mut rows = rows.into_iter();
    if !options.header {
        return Ok(rows
            .map(|r| Value::Array(r.into_iter().map(Value::String).collect()))
            .collect());
    }
    let names: Vec<String> = rows.next().unwrap_or_default();
    Ok(rows
        .map(|record| {
            let mut map = Map::new();
            for (i, value) in record.into_iter().enumerate() {
                let key = names
                    .get(i)
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("column{}", i + 1));
                map.insert(key, Value::String(value));
            }
            Value::Object(map)
        })
        .collect())
}

fn quote(field: &str, delimiter: char) -> String {
    if field.contains([delimiter, '"', '\n', '\r'])
        || field.starts_with(' ')
        || field.ends_with(' ')
    {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => quench_serde::json::to_string(other),
    }
}

/// Writes records.
///
/// Objects become rows under a header made of the keys of the first object (a later
/// object is written in that column order, with an empty field where it lacks a key); arrays are rows
/// as they are. Strings are written as they are, `null` as an empty field, anything else as JSON.
#[must_use]
pub fn to_string(rows: &[Value], delimiter: u8) -> String {
    let d = char::from(delimiter);
    let mut out = String::new();
    let header: Option<Vec<String>> = match rows.first() {
        Some(Value::Object(map)) => Some(map.keys().cloned().collect()),
        _ => None,
    };
    let mut line = |fields: Vec<String>| {
        out.push_str(
            &fields
                .iter()
                .map(|f| quote(f, d))
                .collect::<Vec<_>>()
                .join(&d.to_string()),
        );
        out.push('\n');
    };
    if let Some(names) = &header {
        line(names.clone());
    }
    for row in rows {
        match (row, &header) {
            (Value::Object(map), Some(names)) => {
                line(
                    names
                        .iter()
                        .map(|n| map.get(n).map(text).unwrap_or_default())
                        .collect(),
                );
            }
            (Value::Array(items), _) => line(items.iter().map(text).collect()),
            (other, _) => line(vec![text(other)]),
        }
    }
    out
}
