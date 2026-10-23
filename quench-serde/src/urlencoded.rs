//! `application/x-www-form-urlencoded`: query strings and form bodies.
//!
//! Every value is text, so reading into a typed struct parses it (`n=5` into
//! a `u32`). A key that repeats becomes a list. Writing handles flat structs
//! and maps, lists as repeated keys, and `None` as an absent key; nested
//! objects have no form and are an error.

use std::fmt::Write as _;

use crate::{Deserialize, Error, Map, Result, Serialize, Value};

/// Reads a `T` from `a=1&b=two` text.
pub fn from_str<T: Deserialize>(text: &str) -> Result<T> {
    let value = parse(text);
    let _lenient = crate::private::Lenient::enter();
    T::deserialize(&value)
}

/// Like [`from_str`] for bytes, which are decoded as UTF-8 (invalid sequences
/// become U+FFFD, as browsers do).
pub fn from_bytes<T: Deserialize>(bytes: &[u8]) -> Result<T> {
    from_str(&String::from_utf8_lossy(bytes))
}

/// The pairs in `a=1&b=two`, decoded, in order.
#[must_use]
pub fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode(key), decode(value))
        })
        .collect()
}

/// The text as an object of strings; a repeated key becomes an array.
fn parse(text: &str) -> Value {
    let mut map = Map::new();
    for (key, value) in parse_pairs(text) {
        match map.get_mut(&key) {
            None => {
                map.insert(key, Value::String(value));
            }
            Some(Value::Array(items)) => items.push(Value::String(value)),
            Some(existing) => {
                let first = existing.take();
                *existing = Value::Array(vec![first, Value::String(value)]);
            }
        }
    }
    Value::Object(map)
}

/// Writes `a=1&b=two` from a struct, a map, or a list of `(key, value)` pairs.
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String> {
    let mut out = String::new();
    let mut add = |key: &str, text: &str| {
        if !out.is_empty() {
            out.push('&');
        }
        out.push_str(&encode(key));
        out.push('=');
        out.push_str(&encode(text));
    };
    match value.serialize() {
        Value::Object(map) => {
            for (key, item) in &map {
                emit(key, item, &mut add)?;
            }
        }
        Value::Array(pairs) => {
            for pair in &pairs {
                match pair.as_array().map(Vec::as_slice) {
                    Some([key, item]) => emit(&scalar(key)?, item, &mut add)?,
                    _ => return Err(Error::new("a form list must hold (key, value) pairs")),
                }
            }
        }
        Value::Null => {}
        other => {
            return Err(crate::invalid_type(
                "a map, struct or list of pairs",
                &other,
            ));
        }
    }
    Ok(out)
}

fn emit(key: &str, value: &Value, add: &mut impl FnMut(&str, &str)) -> Result<()> {
    match value {
        Value::Null => Ok(()),
        Value::Array(items) => {
            for item in items {
                if !item.is_null() {
                    add(key, &scalar(item).map_err(|e| e.in_field(key))?);
                }
            }
            Ok(())
        }
        other => {
            add(key, &scalar(other).map_err(|e| e.in_field(key))?);
            Ok(())
        }
    }
}

fn scalar(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Int(n) => Ok(n.to_string()),
        Value::UInt(n) => Ok(n.to_string()),
        Value::Float(f) => Ok(crate::json::stringify(&Value::Float(*f))),
        other => Err(Error::new(format!(
            "a {} cannot be written as a form value",
            other.type_name()
        ))),
    }
}

const fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'*' | b'-' | b'.' | b'_')
}

/// Form-encodes text: a space is `+`, unreserved bytes stay, the rest is `%XX`.
#[must_use]
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        match byte {
            b' ' => out.push('+'),
            b if unreserved(b) => out.push(char::from(b)),
            b => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// Percent-encodes text for a URL path segment or query value (RFC 3986).
///
/// A space is `%20`, the unreserved characters `A-Z a-z 0-9 - . _ ~` stay and every other byte is `%XX`.
/// Unlike [`encode`] there is no `+`, which in a path is just a plus.
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Reverses [`percent_encode`]: `%XX` is a byte and `+` stays a plus; a malformed escape is kept as written
/// and invalid UTF-8 becomes U+FFFD.
#[must_use]
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(&hi), Some(&lo)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(hi), Some(lo)) = (hex(hi), hex(lo))
        {
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// Reverses [`encode`]: `+` is a space, `%XX` is a byte; a malformed escape is
/// kept as written and invalid UTF-8 becomes U+FFFD.
#[must_use]
pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                if let (Some(&hi), Some(&lo)) = (bytes.get(i + 1), bytes.get(i + 2))
                    && let (Some(hi), Some(lo)) = (hex(hi), hex(lo))
                {
                    out.push(hi << 4 | lo);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn encodes_like_a_form() {
        assert_eq!(encode("a b&c=d/é*-._~"), "a+b%26c%3Dd%2F%C3%A9*-._%7E");
        assert_eq!(encode(""), "");
    }

    #[test]
    fn decodes_escapes_plus_and_leaves_bad_ones() {
        assert_eq!(decode("a+b%26c%3dd%C3%A9"), "a b&c=dé");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz%4"), "%zz%4");
        assert_eq!(decode("%FF"), "\u{fffd}");
        assert_eq!(decode(""), "");
    }

    #[test]
    fn round_trips_every_byte_value() {
        let all: String = (1..=0xd7ff_u32)
            .step_by(7)
            .filter_map(char::from_u32)
            .collect();
        assert_eq!(decode(&encode(&all)), all);
    }

    #[test]
    fn pairs() {
        assert_eq!(
            parse_pairs("a=1&b=&c&&d=x%20y"),
            [("a", "1"), ("b", ""), ("c", ""), ("d", "x y")]
                .map(|(k, v)| (k.to_string(), v.to_string()))
        );
        assert!(parse_pairs("").is_empty());
        assert_eq!(parse_pairs("a=b=c")[0].1, "b=c");
    }

    #[derive(Debug, PartialEq, crate::Serialize, crate::Deserialize)]
    struct Form {
        name: String,
        age: u32,
        admin: bool,
        score: f64,
        nick: Option<String>,
        tags: Vec<String>,
    }

    #[test]
    fn reads_typed_structs_from_text() {
        let form: Form =
            from_str("name=Ada+L&age=36&admin=true&score=1.5&tags=a&tags=b&tags=c").unwrap();
        assert_eq!(
            form,
            Form {
                name: "Ada L".into(),
                age: 36,
                admin: true,
                score: 1.5,
                nick: None,
                tags: vec!["a".into(), "b".into(), "c".into()]
            }
        );
        let form: Form = from_bytes(b"name=x&age=1&admin=false&score=2&nick=n&tags=only").unwrap();
        assert_eq!(form.nick.as_deref(), Some("n"));
        assert_eq!(form.tags, ["only"]);
    }

    #[test]
    fn type_errors_name_the_field() {
        let e = from_str::<Form>("name=x&age=old&admin=true&score=1&tags=a&tags=b").unwrap_err();
        assert_eq!(e.path(), "age");
        assert!(e.to_string().contains("invalid integer"), "{e}");
        let e = from_str::<Form>("age=1").unwrap_err();
        assert_eq!(e.to_string(), "missing field `name`");
    }

    #[test]
    fn json_stays_strict_after_a_form_read() {
        let _ = from_str::<Form>("name=x");
        assert!(crate::json::from_str::<u8>("\"5\"").is_err());
    }

    #[test]
    fn maps_and_pair_lists() {
        let map: HashMap<String, String> = from_str("a=1&b=2").unwrap();
        assert_eq!(map["b"], "2");
        let pairs: Vec<(String, String)> = parse_pairs("x=1&y=2");
        assert_eq!(to_string(&pairs).unwrap(), "x=1&y=2");
        let sorted: BTreeMap<&str, u8> = BTreeMap::from([("b", 2), ("a", 1)]);
        assert_eq!(to_string(&sorted).unwrap(), "a=1&b=2");
        assert_eq!(to_string(&[("k", "v w")]).unwrap(), "k=v+w");
    }

    #[test]
    fn writes_structs() {
        let form = Form {
            name: "Ada L".into(),
            age: 36,
            admin: true,
            score: 1.5,
            nick: None,
            tags: vec!["a".into(), "b&c".into()],
        };
        assert_eq!(
            to_string(&form).unwrap(),
            "name=Ada+L&age=36&admin=true&score=1.5&tags=a&tags=b%26c"
        );
        assert_eq!(to_string(&()).unwrap(), "");
    }

    #[test]
    fn writes_every_scalar_kind() {
        let value = crate::json!({"s": "x", "b": false, "i": -3, "u": 3, "f": 0.5, "n": null, "list": [1, null, 2]});
        assert_eq!(
            to_string(&value).unwrap(),
            "s=x&b=false&i=-3&u=3&f=0.5&list=1&list=2"
        );
    }

    #[test]
    fn unwritable_shapes_are_errors() {
        let nested = crate::json!({"a": {"b": 1}});
        let e = to_string(&nested).unwrap_err();
        assert_eq!(e.path(), "a");
        assert!(e.to_string().contains("object cannot be written"), "{e}");
        assert!(to_string(&crate::json!({"a": [[1]]})).is_err());
        assert!(
            to_string(&5)
                .unwrap_err()
                .to_string()
                .contains("expected a map")
        );
        assert!(
            to_string(&crate::json!([1, 2]))
                .unwrap_err()
                .to_string()
                .contains("(key, value) pairs")
        );
        assert!(to_string(&crate::json!([[{"x": 1}, 2]])).is_err());
    }

    #[test]
    fn percent_encoding_is_for_paths_not_forms() {
        assert_eq!(percent_encode("a b&c/d+e~f"), "a%20b%26c%2Fd%2Be~f");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(encode("a b"), "a+b", "form encoding is unchanged");
        assert_eq!(percent_decode("a%20b%2Bc+d"), "a b+c+d");
        assert_eq!(percent_decode("%C3%A9%zz%4"), "é%zz%4");
        assert_eq!(percent_decode(&percent_encode("x y/z?=&#é")), "x y/z?=&#é");
    }
}
