//! The YAML writer: block style, two-space indentation, sequences flush with
//! their key (the layout `serde_yaml` and `kubectl` produce).

use std::fmt::Write as _;

use crate::{Result, Serialize, Value};

use super::resolve;

/// Writes one document (no `---` line).
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let mut out = String::new();
    write_document(&mut out, &value.serialize())?;
    Ok(out)
}

/// Writes several documents separated by `---` lines.
pub fn to_string_all<T: Serialize>(values: &[T]) -> Result<String> {
    let mut out = String::new();
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push_str("---\n");
        }
        write_document(&mut out, &value.serialize())?;
    }
    Ok(out)
}

fn write_document(out: &mut String, value: &Value) -> Result<()> {
    match value {
        Value::Object(map) if !map.is_empty() => block_mapping(out, value, 0),
        Value::Array(items) if !items.is_empty() => block_sequence(out, value, 0),
        // A root block scalar is indented one level, like everything nested.
        Value::String(text) if text.contains('\n') => {
            string(out, text, 1);
            out.push('\n');
            Ok(())
        }
        other => {
            scalar(out, other, 0);
            out.push('\n');
            Ok(())
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    out.push_str(&"  ".repeat(depth));
}

fn block_mapping(out: &mut String, value: &Value, depth: usize) -> Result<()> {
    let Value::Object(map) = value else {
        return Ok(());
    };
    for (key, item) in map {
        indent(out, depth);
        out.push_str(&key_text(key));
        out.push(':');
        entry_value(out, item, depth, depth + 1)?;
    }
    Ok(())
}

fn block_sequence(out: &mut String, value: &Value, depth: usize) -> Result<()> {
    let Value::Array(items) = value else {
        return Ok(());
    };
    for item in items {
        indent(out, depth);
        out.push('-');
        match item {
            Value::Object(m) if !m.is_empty() => {
                // `- key: v` with the rest of the keys lined up under the first
                out.push(' ');
                let mut first = true;
                for (key, inner) in m {
                    if !first {
                        indent(out, depth + 1);
                    }
                    first = false;
                    out.push_str(&key_text(key));
                    out.push(':');
                    entry_value(out, inner, depth + 1, depth + 2)?;
                }
            }
            Value::Array(a) if !a.is_empty() => {
                out.push(' ');
                // nested sequence: first item on the dash line
                let mut inner = String::new();
                block_sequence(&mut inner, item, depth + 1)?;
                out.push_str(inner.trim_start_matches(' '));
            }
            other => {
                out.push(' ');
                scalar(out, other, depth + 1);
                out.push('\n');
            }
        }
    }
    Ok(())
}

/// What follows `key:`. `owner` is the depth of the key; `child` where nested
/// block content goes.
fn entry_value(out: &mut String, value: &Value, owner: usize, child: usize) -> Result<()> {
    match value {
        Value::Object(m) if !m.is_empty() => {
            out.push('\n');
            block_mapping(out, value, child)
        }
        Value::Array(a) if !a.is_empty() => {
            out.push('\n');
            // sequences sit at the key's own depth
            block_sequence(out, value, owner)
        }
        other => {
            out.push(' ');
            scalar(out, other, child);
            out.push('\n');
            Ok(())
        }
    }
}

/// A scalar or empty collection on one line (block scalars span lines).
fn scalar(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::UInt(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(f) if f.is_nan() => out.push_str(".nan"),
        Value::Float(f) if f.is_infinite() => out.push_str(if *f > 0.0 { ".inf" } else { "-.inf" }),
        // YAML spells the exponent `e20`, not `e+20`.
        Value::Float(f) => out.push_str(&crate::json::float_text(*f).replace("e+", "e")),
        Value::String(s) => string(out, s, depth),
        Value::Array(_) => out.push_str("[]"),
        Value::Object(_) => out.push_str("{}"),
    }
}

fn key_text(key: &str) -> String {
    let mut out = String::new();
    if key.contains('\n') {
        double_quoted(&mut out, key);
    } else {
        string(&mut out, key, 0);
    }
    out
}

/// Plain when that reads back as the same string, else quoted; text with line
/// breaks becomes a `|` block.
fn string(out: &mut String, text: &str, depth: usize) {
    if text.contains('\n')
        && !text.chars().any(|c| c.is_control() && c != '\n')
        && !text.starts_with([' ', '\n'])
    {
        let chomp = if text.ends_with("\n\n") {
            "+"
        } else if text.ends_with('\n') {
            ""
        } else {
            "-"
        };
        out.push('|');
        out.push_str(chomp);
        let body = if chomp == "-" {
            text
        } else {
            text.strip_suffix('\n').unwrap_or(text)
        };
        for line in body.split('\n') {
            out.push('\n');
            if !line.is_empty() {
                indent(out, depth);
                out.push_str(line);
            }
        }
        return;
    }
    if is_plain_safe(text) {
        out.push_str(text);
    } else if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        || text.contains('\n')
    {
        double_quoted(out, text);
    } else {
        out.push('\'');
        out.push_str(&text.replace('\'', "''"));
        out.push('\'');
    }
}

fn double_quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() && u32::from(c) <= 0xff => {
                let _ = write!(out, "\\x{:02X}", u32::from(c));
            }
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Would `text` read back as this exact string if written bare?
fn is_plain_safe(text: &str) -> bool {
    if text.is_empty() || text.starts_with([' ', '\t']) || text.ends_with([' ', '\t']) {
        return false;
    }
    if !matches!(resolve::plain(text), Value::String(_)) {
        return false;
    }
    let first = text.as_bytes()[0];
    if matches!(
        first,
        b',' | b'['
            | b']'
            | b'{'
            | b'}'
            | b'#'
            | b'&'
            | b'*'
            | b'!'
            | b'|'
            | b'>'
            | b'\''
            | b'"'
            | b'%'
            | b'@'
            | b'`'
    ) {
        return false;
    }
    if matches!(first, b'-' | b'?' | b':')
        && text.as_bytes().get(1).is_none_or(u8::is_ascii_whitespace)
    {
        return false;
    }
    if text.contains(": ") || text.contains(" #") || text.ends_with(':') {
        return false;
    }
    text.chars()
        .all(|c| !c.is_control() && !matches!(c, '\u{feff}' | '\u{2028}' | '\u{2029}'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json;

    fn j(text: &str) -> Value {
        json::parse(text).unwrap()
    }

    fn out(text: &str) -> String {
        to_string(&j(text)).unwrap()
    }

    #[quench_test::test]
    fn scalars() {
        assert_eq!(out("null"), "null\n");
        assert_eq!(out("true"), "true\n");
        assert_eq!(out("-5"), "-5\n");
        assert_eq!(out("5"), "5\n");
        assert_eq!(out("1.0"), "1.0\n");
        assert_eq!(out("1e20"), "1e20\n");
        assert_eq!(out("\"text\""), "text\n");
        assert_eq!(out("[]"), "[]\n");
        assert_eq!(out("{}"), "{}\n");
        assert_eq!(to_string(&Value::Float(f64::INFINITY)).unwrap(), ".inf\n");
        assert_eq!(
            to_string(&Value::Float(f64::NEG_INFINITY)).unwrap(),
            "-.inf\n"
        );
        assert_eq!(to_string(&Value::Float(f64::NAN)).unwrap(), ".nan\n");
    }

    #[quench_test::test]
    fn block_layout_matches_serde_yaml() {
        let value = j(
            r#"{"name":"x","list":["a",{"b":1,"c":[1,2]},["n1","n2"]],"empty_map":{},"empty_seq":[],"nested":{"k":{"deep":true}}}"#,
        );
        assert_eq!(
            to_string(&value).unwrap(),
            "name: x\nlist:\n- a\n- b: 1\n  c:\n  - 1\n  - 2\n- - n1\n  - n2\nempty_map: {}\nempty_seq: []\nnested:\n  k:\n    deep: true\n"
        );
    }

    #[quench_test::test]
    fn sequence_documents() {
        assert_eq!(out(r#"[1,[2,3],{"a":1}]"#), "- 1\n- - 2\n  - 3\n- a: 1\n");
        assert_eq!(out(r#"[{"a":1,"b":{"c":2}}]"#), "- a: 1\n  b:\n    c: 2\n");
    }

    #[quench_test::test]
    fn strings_are_quoted_exactly_when_needed() {
        let plain = [
            "plain",
            "two words",
            "a-b",
            "a:b",
            "http://x/y",
            "a#b",
            "-x",
            "é中😀",
            "a'b",
            "=",
            "<<",
            "007",
            "1_000",
            "0x",
            "yes",
            "no",
            "on",
        ];
        for text in plain {
            assert_eq!(
                to_string(&Value::from(text)).unwrap(),
                format!("{text}\n"),
                "{text}"
            );
        }
        let single = [
            "", " lead", "trail ", "true", "null", "~", "123", "1.5", "0x1f", "- dash", "? q",
            ": c", "a: b", "a #c", "#start", "&anchor", "*alias", "!tag", "|pipe", ">gt", "'sq'",
            "\"dq\"", "%pct", "@at", "`tick", "{a}", "[a]", ",x", "colon:", ".inf", "-.5", "+1",
            "1e3",
        ];
        for text in single {
            let written = to_string(&Value::from(text)).unwrap();
            assert!(written.starts_with('\''), "{text:?} -> {written}");
            assert_eq!(
                crate::yaml::parse(&written).unwrap(),
                Value::from(text),
                "{text:?}"
            );
        }
        assert_eq!(to_string(&Value::from("it's: x")).unwrap(), "'it''s: x'\n");
    }

    #[quench_test::test]
    fn control_characters_and_odd_whitespace_use_double_quotes() {
        assert_eq!(
            to_string(&Value::from("ctl\u{1}x")).unwrap(),
            "\"ctl\\x01x\"\n"
        );
        assert_eq!(
            to_string(&Value::from("bell\u{7f}")).unwrap(),
            "\"bell\\x7F\"\n"
        );
        assert_eq!(to_string(&Value::from("line\r")).unwrap(), "\"line\\r\"\n");
        assert_eq!(
            to_string(&Value::from("tab\there")).unwrap(),
            "\"tab\\there\"\n"
        );
        assert_eq!(
            to_string(&Value::from("\u{2028}")).unwrap(),
            "\"\\u2028\"\n"
        );
        assert_eq!(to_string(&Value::from("q\"b\\")).unwrap(), "q\"b\\\n");
        let both = to_string(&Value::from("quote ' and \u{1}")).unwrap();
        assert_eq!(both, "\"quote ' and \\x01\"\n");
    }

    #[quench_test::test]
    fn multiline_strings_become_block_scalars() {
        assert_eq!(out(r#"{"m":"line1\nline2\n"}"#), "m: |\n  line1\n  line2\n");
        assert_eq!(out(r#"{"m":"a\nb"}"#), "m: |-\n  a\n  b\n");
        assert_eq!(out(r#"{"m":"a\n\n"}"#), "m: |+\n  a\n\n");
        assert_eq!(out(r#"{"m":"a\n\nb\n"}"#), "m: |\n  a\n\n  b\n");
        assert_eq!(out(r#"{"l":["x\ny"]}"#), "l:\n- |-\n  x\n  y\n");
        assert_eq!(out(r#"{"a":{"b":"x\ny"}}"#), "a:\n  b: |-\n    x\n    y\n");
        assert_eq!(out(r#""root\nline""#), "|-\n  root\n  line\n");
        // leading space or break, or a control character, cannot be a block scalar
        assert_eq!(out(r#"{"m":" lead\nx"}"#), "m: \" lead\\nx\"\n");
        assert_eq!(out(r#"{"m":"\nlead"}"#), "m: \"\\nlead\"\n");
        assert_eq!(out(r#"{"m":"a\u0001\nb"}"#), "m: \"a\\x01\\nb\"\n");
    }

    #[quench_test::test]
    fn keys_are_quoted_like_values() {
        assert_eq!(
            out(r#"{"key with space":1,"a: b":2,"":3,"true":4,"x\ny":5}"#),
            "key with space: 1\n'a: b': 2\n'': 3\n'true': 4\n\"x\\ny\": 5\n"
        );
    }

    #[quench_test::test]
    fn everything_round_trips() {
        let docs = [
            r#"{"a":[1,2,{"b":[true,null,"x"],"c":{}}],"d":"multi\nline","e":"  spaced  ","f":["- x","? y",": z"]}"#,
            r#"[[1,[2,[3,[]]]],{"k":[{"a":[]}]}]"#,
            r#"{"z":{"y":{"x":{"w":"v"}}},"n":-0.5,"big":18446744073709551615,"small":-9223372036854775808}"#,
            r#"{"s":["","  ","\t","\n","a\n","\na"]}"#,
        ];
        for doc in docs {
            let value = j(doc);
            let text = to_string(&value).unwrap();
            assert_eq!(crate::yaml::parse(&text).unwrap(), value, "{text}");
        }
    }

    #[quench_test::test]
    fn multiple_documents() {
        let text = to_string_all(&[j(r#"{"a":1}"#), j("[1,2]"), j(r#""s""#)]).unwrap();
        assert_eq!(text, "a: 1\n---\n- 1\n- 2\n---\ns\n");
        assert_eq!(crate::yaml::parse_all(&text).unwrap().len(), 3);
        assert_eq!(to_string_all::<Value>(&[]).unwrap(), "");
    }
}
