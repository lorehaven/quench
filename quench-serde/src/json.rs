//! JSON (RFC 8259) to and from [`Value`].
//!
//! The parser is strict: no comments, trailing commas, leading zeros, single
//! quotes, unescaped control characters, lone surrogates or trailing content.
//! Nesting is capped at [`MAX_DEPTH`] so hostile input cannot exhaust the stack.

use std::fmt::Write as _;

use crate::{Deserialize, Error, Map, Result, Serialize, Value};

/// How deeply arrays and objects may nest.
pub const MAX_DEPTH: usize = 128;

/// Parses one JSON document.
pub fn parse(text: &str) -> Result<Value> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_ws();
    let value = parser.value(0)?;
    parser.skip_ws();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("trailing characters after the JSON value"));
    }
    Ok(value)
}

/// Like [`parse`] for bytes, which must be UTF-8.
pub fn parse_slice(bytes: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| Error::new(format!("invalid UTF-8 in JSON at byte {}", e.valid_up_to())))?;
    parse(text)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> Error {
        Error::at(message, self.text, self.pos)
    }

    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn value(&mut self, depth: usize) -> Result<Value> {
        match self.peek() {
            None => Err(self.error("unexpected end of input")),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("expected a JSON value")),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value> {
        if depth >= MAX_DEPTH {
            return Err(self.error("recursion limit exceeded"));
        }
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value> {
        if depth >= MAX_DEPTH {
            return Err(self.error("recursion limit exceeded"));
        }
        self.pos += 1;
        let mut map = Map::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a string key"));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(self.error("expected `:`"));
            }
            self.pos += 1;
            self.skip_ws();
            // A repeated key keeps the last value, like every JSON library.
            map.insert(key, self.value(depth + 1)?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Object(map));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.pos;
        let negative = self.peek() == Some(b'-');
        if negative {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error("invalid number")),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(self.error("expected digits after the decimal point"));
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(self.error("expected digits in the exponent"));
            }
            self.digits();
        }
        let text = &self.text[start..self.pos];
        if !is_float {
            if negative {
                match text.parse::<i64>() {
                    // `-0` is a negative zero, which an integer cannot hold.
                    Ok(0) => return Ok(Value::Float(-0.0)),
                    Ok(n) => return Ok(Value::Int(n)),
                    Err(_) => {}
                }
            } else if let Ok(n) = text.parse::<u64>() {
                return Ok(Value::UInt(n));
            }
        }
        match text.parse::<f64>() {
            Ok(f) if f.is_finite() => Ok(Value::Float(f)),
            _ => Err(Error::at("number out of range", self.text, start)),
        }
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.pos += 1;
        }
    }

    fn string(&mut self) -> Result<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            // Copy the run up to the next quote, backslash or control character.
            let run_start = self.pos;
            while self
                .peek()
                .is_some_and(|b| b != b'"' && b != b'\\' && b >= 0x20)
            {
                self.pos += 1;
            }
            out.push_str(&self.text[run_start..self.pos]);
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                Some(_) => return Err(self.error("unescaped control character in string")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<()> {
        let Some(byte) = self.peek() else {
            return Err(self.error("unterminated escape"));
        };
        self.pos += 1;
        let simple = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let unit = self.hex4()?;
                let ch = match unit {
                    0xD800..=0xDBFF => {
                        if !self.bytes[self.pos..].starts_with(b"\\u") {
                            return Err(self.error("unpaired high surrogate"));
                        }
                        self.pos += 2;
                        let low = self.hex4()?;
                        if !(0xDC00..=0xDFFF).contains(&low) {
                            return Err(self.error("invalid low surrogate"));
                        }
                        char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00))
                    }
                    0xDC00..=0xDFFF => return Err(self.error("unpaired low surrogate")),
                    _ => char::from_u32(unit),
                };
                out.push(ch.ok_or_else(|| self.error("invalid unicode escape"))?);
                return Ok(());
            }
            _ => {
                self.pos -= 1;
                return Err(self.error("invalid escape"));
            }
        };
        out.push(simple);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("truncated unicode escape"))?;
        let mut unit = 0u32;
        for &digit in digits {
            let value = char::from(digit)
                .to_digit(16)
                .ok_or_else(|| self.error("invalid unicode escape"))?;
            unit = unit * 16 + value;
        }
        self.pos += 4;
        Ok(unit)
    }
}

/// Reads a `T` from JSON text.
pub fn from_str<T: Deserialize>(text: &str) -> Result<T> {
    T::deserialize(&parse(text)?)
}

/// Reads a `T` from UTF-8 JSON bytes.
pub fn from_slice<T: Deserialize>(bytes: &[u8]) -> Result<T> {
    T::deserialize(&parse_slice(bytes)?)
}

/// Compact JSON: `{"a":[1,2]}`.
#[must_use]
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> String {
    stringify(&value.serialize())
}

/// Two-space indented JSON, one element per line.
#[must_use]
pub fn to_string_pretty<T: Serialize + ?Sized>(value: &T) -> String {
    stringify_pretty(&value.serialize())
}

#[must_use]
pub fn to_vec<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    to_string(value).into_bytes()
}

/// Compact JSON for a value already in hand (no conversion, no copy).
pub(crate) fn stringify(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, None, 0);
    out
}

pub(crate) fn stringify_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, Some("  "), 0);
    out
}

fn newline(out: &mut String, indent: Option<&str>, level: usize) {
    if let Some(unit) = indent {
        out.push('\n');
        for _ in 0..level {
            out.push_str(unit);
        }
    }
}

fn write_value(out: &mut String, value: &Value, indent: Option<&str>, level: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(f) => write_float(out, *f),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, level + 1);
                write_value(out, item, indent, level + 1);
            }
            newline(out, indent, level);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, level + 1);
                write_string(out, key);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, item, indent, level + 1);
            }
            newline(out, indent, level);
            out.push('}');
        }
    }
}

/// A float as JSON writes it (`1.0`, `0.00001`, `1e+20`); `null` if not finite.
#[must_use]
pub fn float_text(f: f64) -> String {
    let mut out = String::new();
    write_float(&mut out, f);
    out
}

/// Shortest text that reads back as the same float, laid out the way
/// `serde_json` does: plain decimal from `1e-5` up to (not including) `1e16`
/// with a `.0` on whole numbers, scientific (`1e+20`, `5e-324`) beyond that.
pub(crate) fn write_float(out: &mut String, f: f64) {
    if !f.is_finite() {
        out.push_str("null");
        return;
    }
    // `{:e}` gives the shortest round-tripping digits: `-1.2345e2`.
    let sci = format!("{f:e}");
    let (mantissa, exponent) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = mantissa
        .strip_prefix('-')
        .map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    out.push_str(sign);
    if (-5..16).contains(&exponent) {
        if let (true, Ok(whole)) = (exponent >= 0, usize::try_from(exponent + 1)) {
            if digits.len() <= whole {
                out.push_str(&digits);
                out.push_str(&"0".repeat(whole - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..whole]);
                out.push('.');
                out.push_str(&digits[whole..]);
            }
        } else {
            out.push_str("0.");
            out.push_str(&"0".repeat(usize::try_from(-exponent - 1).unwrap_or(0)));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let _ = write!(out, "e{}{exponent}", if exponent > 0 { "+" } else { "" });
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c < ' ' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(text: &str) -> Value {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
    }

    fn rejects(text: &str) -> Error {
        parse(text).expect_err(text)
    }

    #[test]
    fn scalars() {
        assert_eq!(ok("null"), Value::Null);
        assert_eq!(ok("true"), Value::Bool(true));
        assert_eq!(ok("false"), Value::Bool(false));
        assert_eq!(ok("0"), Value::UInt(0));
        assert_eq!(ok("42"), Value::UInt(42));
        assert_eq!(ok("-42"), Value::Int(-42));
        assert_eq!(ok("1.5"), Value::Float(1.5));
        assert_eq!(ok("-1.5e2"), Value::Float(-150.0));
        assert_eq!(ok("1E3"), Value::Float(1000.0));
        assert_eq!(ok("1e+3"), Value::Float(1000.0));
        assert_eq!(ok("2.5e-3"), Value::Float(0.0025));
        assert_eq!(ok("\"hi\""), Value::from("hi"));
        assert_eq!(ok(" \t\r\n 7 \n"), Value::UInt(7));
    }

    #[test]
    fn integer_edges() {
        assert_eq!(ok("18446744073709551615"), Value::UInt(u64::MAX));
        assert_eq!(ok("-9223372036854775808"), Value::Int(i64::MIN));
        // beyond the 64-bit range an integer becomes a float
        assert_eq!(
            ok("18446744073709551616"),
            Value::Float(18_446_744_073_709_551_616.0)
        );
        assert_eq!(
            ok("-9223372036854775809"),
            Value::Float(-9_223_372_036_854_775_809.0)
        );
        // negative zero cannot be an integer
        assert!(matches!(ok("-0"), Value::Float(f) if f == 0.0 && f.is_sign_negative()));
        assert!(matches!(ok("-0.0"), Value::Float(f) if f.is_sign_negative()));
    }

    #[test]
    fn rejects_bad_numbers() {
        for bad in [
            "01",
            "-",
            "+1",
            ".5",
            "1.",
            "1.e3",
            "1e",
            "1e+",
            "-a",
            "0x10",
            "1_000",
            "--1",
            "1.2.3",
            "NaN",
            "Infinity",
            "-Infinity",
            "1e999",
            "-1e999",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(rejects("1e999").to_string().contains("out of range"));
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            ok(r#""a\"b\\c\/d\b\f\n\r\t""#),
            Value::from("a\"b\\c/d\u{8}\u{c}\n\r\t")
        );
        assert_eq!(ok(r#""Aé中""#), Value::from("Aé中"));
        assert_eq!(ok(r#""😀""#), Value::from("😀"));
        assert_eq!(ok("\"raw é 中 😀\""), Value::from("raw é 中 😀"));
        assert_eq!(ok(r#""\u0000""#), Value::from("\0"));
        assert_eq!(ok(r#""""#), Value::from(""));
    }

    #[test]
    fn rejects_bad_strings() {
        for bad in [
            "\"abc",
            "\"a\nb\"",
            "\"a\tb\"",
            "\"\u{1}\"",
            r#""\x41""#,
            r#""\u12""#,
            r#""\u12g4""#,
            r#""\ud83d""#,
            r#""\ud83dx""#,
            r#""\ud83dA""#,
            r#""\ude00""#,
            "\"\\",
            "'single'",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn arrays_and_objects() {
        assert_eq!(ok("[]"), Value::Array(vec![]));
        assert_eq!(
            ok("[ 1 , [ ] , \"x\" ]"),
            Value::Array(vec![Value::UInt(1), Value::Array(vec![]), Value::from("x")])
        );
        assert_eq!(ok("{}"), Value::Object(Map::new()));
        let v = ok(r#"{"b":1,"a":{"c":[true,null]}}"#);
        assert_eq!(v["a"]["c"][0], Value::Bool(true));
        // order is kept
        assert_eq!(
            v.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
    }

    #[test]
    fn duplicate_keys_keep_the_last_value_in_the_first_position() {
        let v = ok(r#"{"a":1,"b":2,"a":3}"#);
        assert_eq!(v["a"], Value::UInt(3));
        assert_eq!(
            v.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn rejects_structural_errors() {
        for bad in [
            "",
            " ",
            "[",
            "]",
            "{",
            "}",
            "[1,]",
            "[,1]",
            "[1 2]",
            "[1,,2]",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "{,}",
            concat!("{a", ":1}"),
            concat!("{1", ":1}"),
            "{\"a\" 1}",
            "{\"a\":1 \"b\":2}",
            "nul",
            "tru",
            "falsy",
            "nulll",
            "[nul]",
            "1 2",
            "{} {}",
            "[1]]",
            "\u{feff}1",
            "/*c*/1",
            "[1]//c",
            "# 1",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn errors_say_where() {
        let e = rejects("{\n  \"a\": ,\n}");
        assert_eq!(e.location(), Some((2, 8)));
        assert!(e.to_string().contains("line 2 column 8"), "{e}");
        assert!(rejects("[1, 2").to_string().contains("expected"));
        assert!(rejects("").to_string().contains("end of input"));
        assert!(rejects("1 x").to_string().contains("trailing"));
    }

    #[test]
    fn depth_is_limited() {
        let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(&nested(MAX_DEPTH)).is_ok());
        assert!(
            rejects(&nested(MAX_DEPTH + 1))
                .to_string()
                .contains("recursion limit")
        );
        let objects = format!(
            "{}0{}",
            "{\"a\":".repeat(MAX_DEPTH + 1),
            "}".repeat(MAX_DEPTH + 1)
        );
        assert!(rejects(&objects).to_string().contains("recursion limit"));
        // far deeper than the stack could take
        assert!(parse(&"[".repeat(100_000)).is_err());
    }

    #[test]
    fn slices_must_be_utf8() {
        assert_eq!(parse_slice(b"[1]").unwrap(), Value::from(vec![1]));
        let e = parse_slice(b"[\"\xff\"]").unwrap_err();
        assert!(e.to_string().contains("UTF-8"), "{e}");
    }

    #[test]
    fn writes_compact_and_pretty() {
        let v = ok(r#"{"a":[1,2,{"b":null}],"c":{},"d":[],"e":"x"}"#);
        assert_eq!(
            to_string(&v),
            r#"{"a":[1,2,{"b":null}],"c":{},"d":[],"e":"x"}"#
        );
        assert_eq!(
            to_string_pretty(&v),
            "{\n  \"a\": [\n    1,\n    2,\n    {\n      \"b\": null\n    }\n  ],\n  \"c\": {},\n  \"d\": [],\n  \"e\": \"x\"\n}"
        );
        assert_eq!(to_vec(&Value::Bool(true)), b"true");
        assert_eq!(to_string_pretty(&Value::Array(vec![])), "[]");
    }

    #[test]
    fn writes_numbers() {
        assert_eq!(to_string(&Value::UInt(u64::MAX)), "18446744073709551615");
        assert_eq!(to_string(&Value::Int(i64::MIN)), "-9223372036854775808");
        assert_eq!(to_string(&Value::Float(1.0)), "1.0");
        assert_eq!(to_string(&Value::Float(-0.5)), "-0.5");
        assert_eq!(to_string(&Value::Float(0.1)), "0.1");
        assert_eq!(to_string(&Value::Float(0.0)), "0.0");
        assert_eq!(to_string(&Value::Float(-0.0)), "-0.0");
        assert_eq!(to_string(&Value::Float(123.456)), "123.456");
        assert_eq!(to_string(&Value::Float(1e15)), "1000000000000000.0");
        assert_eq!(to_string(&Value::Float(1.5e15)), "1500000000000000.0");
        assert_eq!(to_string(&Value::Float(1e16)), "1e+16");
        assert_eq!(to_string(&Value::Float(1.25e20)), "1.25e+20");
        assert_eq!(to_string(&Value::Float(0.00001)), "0.00001");
        assert_eq!(to_string(&Value::Float(0.000_012)), "0.000012");
        assert_eq!(to_string(&Value::Float(1e-6)), "1e-6");
        assert_eq!(to_string(&Value::Float(1.5e-7)), "1.5e-7");
        assert_eq!(to_string(&Value::Float(5e-324)), "5e-324");
        assert_eq!(
            to_string(&Value::Float(f64::MAX)),
            "1.7976931348623157e+308"
        );
        // a hand-built non-finite float still writes valid JSON
        assert_eq!(to_string(&Value::Float(f64::NAN)), "null");
        assert_eq!(to_string(&Value::Float(f64::INFINITY)), "null");
    }

    #[test]
    fn writes_escapes() {
        assert_eq!(
            to_string(&Value::from("a\"b\\c\n\r\t\u{8}\u{c}\u{1}\u{1f}é😀/")),
            r#""a\"b\\c\n\r\t\b\f\u0001\u001fé😀/""#
        );
        let key = Value::Object(std::iter::once(("k\"ey", Value::Null)).collect());
        assert_eq!(to_string(&key), r#"{"k\"ey":null}"#);
    }

    #[test]
    fn round_trips() {
        for text in [
            "null",
            "true",
            "[]",
            "{}",
            "[1,-2,3.5,\"x\",null,true,[[]],{\"a\":{}}]",
            "{\"z\":1,\"a\":2}",
            "\"\\u0000\\n\"",
            "1e300",
            "5e-324",
            "123456789012345678",
            "-0.0",
        ] {
            let once = ok(text);
            assert_eq!(ok(&to_string(&once)), once, "{text}");
            assert_eq!(ok(&to_string_pretty(&once)), once, "{text}");
        }
    }
}
