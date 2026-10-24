//! Types: the OIDs the client knows, how a text-format column becomes a
//! [`Value`] or a Rust type, and how Rust values are sent as parameters.
//!
//! Everything travels in PostgreSQL's text format. Parameters are sent with
//! type OID 0 so the server infers their type from the statement, and results
//! are converted by the column's OID: numbers become numbers, `json`/`jsonb`
//! are parsed, timestamps become RFC 3339 UTC strings (which is what
//! `quench_core::time::Time` reads), arrays become arrays, and anything
//! unrecognised is its text.

use crate::error::{Error, Result};
use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_serde::Value;
use std::fmt::Write as _;

/// Type OIDs from `pg_type`.
pub mod oid {
    pub const BOOL: u32 = 16;
    pub const BYTEA: u32 = 17;
    pub const CHAR: u32 = 18;
    pub const NAME: u32 = 19;
    pub const INT8: u32 = 20;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const TEXT: u32 = 25;
    pub const OID: u32 = 26;
    pub const XID: u32 = 28;
    pub const JSON: u32 = 114;
    pub const FLOAT4: u32 = 700;
    pub const FLOAT8: u32 = 701;
    pub const BPCHAR: u32 = 1042;
    pub const VARCHAR: u32 = 1043;
    pub const DATE: u32 = 1082;
    pub const TIME: u32 = 1083;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const INTERVAL: u32 = 1186;
    pub const NUMERIC: u32 = 1700;
    pub const UUID: u32 = 2950;
    pub const JSONB: u32 = 3802;
}

/// One result column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub type_oid: u32,
    pub table_oid: u32,
    pub column_id: i16,
    pub type_modifier: i32,
}

/// The element type of a one-dimensional array type, for the common types.
const fn array_element(array_oid: u32) -> Option<u32> {
    Some(match array_oid {
        1000 => oid::BOOL,
        1001 => oid::BYTEA,
        1002 => oid::CHAR,
        1003 => oid::NAME,
        1005 => oid::INT2,
        1007 => oid::INT4,
        1009 => oid::TEXT,
        1014 => oid::BPCHAR,
        1015 => oid::VARCHAR,
        1016 => oid::INT8,
        1021 => oid::FLOAT4,
        1022 => oid::FLOAT8,
        1028 => oid::OID,
        1115 => oid::TIMESTAMP,
        1182 => oid::DATE,
        1183 => oid::TIME,
        1185 => oid::TIMESTAMPTZ,
        1231 => oid::NUMERIC,
        199 => oid::JSON,
        2951 => oid::UUID,
        3807 => oid::JSONB,
        _ => return None,
    })
}

/// Converts a column's text to a [`Value`] according to its type.
#[must_use]
pub fn decode_text(type_oid: u32, text: &str) -> Value {
    match type_oid {
        oid::BOOL => Value::Bool(text == "t"),
        oid::INT2 | oid::INT4 | oid::INT8 => text
            .parse::<i64>()
            .map_or_else(|_| Value::String(text.into()), Value::from_i64),
        oid::OID | oid::XID => text
            .parse::<u64>()
            .map_or_else(|_| Value::String(text.into()), Value::UInt),
        oid::FLOAT4 | oid::FLOAT8 => text
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map_or_else(|| Value::String(text.into()), Value::Float),
        oid::NUMERIC => text
            .parse::<i64>()
            .map(Value::from_i64)
            .or_else(|_| {
                text.parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(Value::Float)
                    .ok_or(())
            })
            .unwrap_or_else(|()| Value::String(text.into())),
        oid::JSON | oid::JSONB => {
            quench_serde::json::parse(text).unwrap_or_else(|_| Value::String(text.into()))
        }
        oid::BYTEA => decode_bytea(text).map_or_else(
            || Value::String(text.into()),
            |bytes| {
                Value::Array(
                    bytes
                        .into_iter()
                        .map(|b| Value::UInt(u64::from(b)))
                        .collect(),
                )
            },
        ),
        oid::TIMESTAMPTZ => parse_timestamp(text, false).map_or_else(
            || Value::String(text.into()),
            |t| Value::String(t.to_rfc3339()),
        ),
        oid::TIMESTAMP => parse_timestamp(text, true).map_or_else(
            || Value::String(text.into()),
            |t| Value::String(t.to_rfc3339()),
        ),
        other => array_element(other)
            .and_then(|element| parse_array(text, element))
            .unwrap_or_else(|| Value::String(text.into())),
    }
}

/// `\xDEADBEEF` (the hex output format) to bytes.
#[must_use]
pub fn decode_bytea(text: &str) -> Option<Vec<u8>> {
    quench_core::hex::decode(text.strip_prefix("\\x")?).ok()
}

/// Bytes to the hex input form `\x..`.
#[must_use]
pub fn encode_bytea(bytes: &[u8]) -> String {
    format!("\\x{}", quench_core::hex::encode(bytes))
}

/// Parses `2026-10-23 01:38:00.123456+02` (or without an offset, taken as UTC when `naive`).
///
/// `BC` dates and `infinity` are not representable and give `None`.
#[must_use]
pub fn parse_timestamp(text: &str, naive: bool) -> Option<Time> {
    let (date, rest) = text.split_once(' ')?;
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    // The clock part ends at the offset sign, if there is one.
    let offset_at = rest.find(['+', '-']);
    let (clock, offset) = offset_at.map_or((rest, None), |i| (&rest[..i], Some(&rest[i..])));
    if !naive && offset.is_none() {
        return None;
    }
    let (hms, fraction) = clock.split_once('.').map_or((clock, ""), |(a, b)| (a, b));
    let mut clock_parts = hms.split(':');
    let hour: u32 = clock_parts.next()?.parse().ok()?;
    let minute: u32 = clock_parts.next()?.parse().ok()?;
    let second: u32 = clock_parts.next()?.parse().ok()?;
    if clock_parts.next().is_some()
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 9
    {
        return None;
    }
    let mut nanos: u32 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    for _ in fraction.len()..9 {
        nanos *= 10;
    }
    let mut seconds = Time::from_ymd_hms(year, month, day, hour, minute, second)?.timestamp();
    if let Some(offset) = offset {
        let sign: i64 = if offset.starts_with('-') { -1 } else { 1 };
        let mut parts = offset[1..].split(':');
        let hours: i64 = parts.next()?.parse().ok()?;
        let minutes: i64 = parts.next().map_or(Some(0), |m| m.parse().ok())?;
        let secs: i64 = parts.next().map_or(Some(0), |s| s.parse().ok())?;
        seconds -= sign * (hours * 3600 + minutes * 60 + secs);
    }
    Some(Time::from_unix_nanos(seconds, nanos))
}

/// Parses a one-dimensional or nested array literal such as `{1,2,3}`,
/// `{"a b",NULL,"say \"hi\""}` or `{{1,2},{3,4}}` (an optional `[1:3]=` bounds prefix is skipped).
#[must_use]
pub fn parse_array(text: &str, element_oid: u32) -> Option<Value> {
    let body = text
        .find('=')
        .filter(|_| text.starts_with('['))
        .map_or(text, |i| &text[i + 1..]);
    let chars: Vec<char> = body.chars().collect();
    let mut pos = 0;
    let value = parse_array_at(&chars, &mut pos, element_oid)?;
    (pos == chars.len()).then_some(value)
}

fn parse_array_at(chars: &[char], pos: &mut usize, element_oid: u32) -> Option<Value> {
    if chars.get(*pos) != Some(&'{') {
        return None;
    }
    *pos += 1;
    let mut items = Vec::new();
    if chars.get(*pos) == Some(&'}') {
        *pos += 1;
        return Some(Value::Array(items));
    }
    loop {
        match chars.get(*pos)? {
            '{' => items.push(parse_array_at(chars, pos, element_oid)?),
            '"' => {
                *pos += 1;
                let mut text = String::new();
                loop {
                    match chars.get(*pos)? {
                        '\\' => {
                            *pos += 1;
                            text.push(*chars.get(*pos)?);
                        }
                        '"' => break,
                        c => text.push(*c),
                    }
                    *pos += 1;
                }
                *pos += 1;
                items.push(decode_text(element_oid, &text));
            }
            _ => {
                let start = *pos;
                while !matches!(chars.get(*pos)?, ',' | '}') {
                    *pos += 1;
                }
                let raw: String = chars[start..*pos].iter().collect();
                let raw = raw.trim();
                items.push(if raw.eq_ignore_ascii_case("null") {
                    Value::Null
                } else {
                    decode_text(element_oid, raw)
                });
            }
        }
        match chars.get(*pos)? {
            ',' => *pos += 1,
            '}' => {
                *pos += 1;
                return Some(Value::Array(items));
            }
            _ => return None,
        }
    }
}

/// A value that can be sent as a statement parameter, in text format.
pub trait ToSql: Sync {
    /// The parameter's text, or `None` for NULL.
    fn to_sql(&self) -> Option<String>;
}

impl ToSql for str {
    fn to_sql(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl ToSql for String {
    fn to_sql(&self) -> Option<String> {
        Some(self.clone())
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> Option<String> {
        Some(if *self { "t" } else { "f" }.to_string())
    }
}

macro_rules! to_sql_display {
    ($($ty:ty),*) => {$(
        impl ToSql for $ty {
            fn to_sql(&self) -> Option<String> {
                Some(self.to_string())
            }
        }
    )*};
}
to_sql_display!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

impl ToSql for f64 {
    fn to_sql(&self) -> Option<String> {
        Some(if self.is_nan() {
            "NaN".to_string()
        } else if self.is_infinite() {
            if *self > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
        } else {
            self.to_string()
        })
    }
}

impl ToSql for f32 {
    fn to_sql(&self) -> Option<String> {
        f64::from(*self).to_sql().map(|text| {
            if self.is_finite() {
                self.to_string()
            } else {
                text
            }
        })
    }
}

impl ToSql for Time {
    fn to_sql(&self) -> Option<String> {
        Some(self.to_rfc3339())
    }
}

impl ToSql for Uuid {
    fn to_sql(&self) -> Option<String> {
        Some(self.to_string())
    }
}

impl ToSql for Value {
    fn to_sql(&self) -> Option<String> {
        Some(quench_serde::json::to_string(self))
    }
}

impl ToSql for [u8] {
    fn to_sql(&self) -> Option<String> {
        Some(encode_bytea(self))
    }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> Option<String> {
        Some(encode_bytea(self))
    }
}

impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_sql(&self) -> Option<String> {
        (**self).to_sql()
    }
}

impl<T: ToSql + ?Sized + Send> ToSql for Box<T> {
    fn to_sql(&self) -> Option<String> {
        (**self).to_sql()
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> Option<String> {
        self.as_ref().and_then(ToSql::to_sql)
    }
}

/// A PostgreSQL array parameter: `PgArray(vec!["a", "b"])` for a `text[]` column or `= ANY($1)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgArray<T>(pub Vec<T>);

impl<T: ToSql> ToSql for PgArray<T> {
    fn to_sql(&self) -> Option<String> {
        let mut out = String::from("{");
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match item.to_sql() {
                None => out.push_str("NULL"),
                Some(text) => {
                    out.push('"');
                    for c in text.chars() {
                        if c == '"' || c == '\\' {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                    out.push('"');
                }
            }
        }
        let _ = write!(out, "}}");
        Some(out)
    }
}

/// A Rust type readable from a result column.
pub trait FromSql: Sized {
    /// Converts a column's text (`None` is NULL).
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] when the value does not fit the type.
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self>;
}

fn not_null<'a>(column: &Column, text: Option<&'a str>) -> Result<&'a str> {
    text.ok_or_else(|| Error::Decode(format!("column `{}` is NULL", column.name)))
}

fn mismatch(column: &Column, expected: &str, text: &str) -> Error {
    Error::Decode(format!(
        "column `{}` (type {}) is `{text}`, not {expected}",
        column.name, column.type_oid
    ))
}

impl FromSql for String {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        not_null(column, text).map(str::to_string)
    }
}

impl FromSql for bool {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        match not_null(column, text)? {
            "t" | "true" => Ok(true),
            "f" | "false" => Ok(false),
            other => Err(mismatch(column, "a boolean", other)),
        }
    }
}

macro_rules! from_sql_parse {
    ($($ty:ty => $what:literal),*) => {$(
        impl FromSql for $ty {
            fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
                let text = not_null(column, text)?;
                text.parse().map_err(|_| mismatch(column, $what, text))
            }
        }
    )*};
}
from_sql_parse!(i16 => "an int2", i32 => "an int4", i64 => "an int8", u32 => "an unsigned 32-bit number", u64 => "an unsigned 64-bit number", f32 => "a float4", f64 => "a float8");

impl FromSql for Time {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        let text = not_null(column, text)?;
        parse_timestamp(text, column.type_oid == oid::TIMESTAMP)
            .ok_or_else(|| mismatch(column, "a timestamp", text))
    }
}

impl FromSql for Uuid {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        let text = not_null(column, text)?;
        Self::parse(text).map_err(|_| mismatch(column, "a uuid", text))
    }
}

impl FromSql for Vec<u8> {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        let text = not_null(column, text)?;
        decode_bytea(text).ok_or_else(|| mismatch(column, "bytea", text))
    }
}

impl FromSql for Value {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        Ok(text.map_or(Self::Null, |text| decode_text(column.type_oid, text)))
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_sql(column: &Column, text: Option<&str>) -> Result<Self> {
        match text {
            None => Ok(None),
            Some(_) => T::from_sql(column, text).map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_serde::json;

    fn col(type_oid: u32) -> Column {
        Column {
            name: "c".into(),
            type_oid,
            table_oid: 0,
            column_id: 0,
            type_modifier: -1,
        }
    }

    #[quench_test::test]
    fn scalars_decode_by_type() {
        assert_eq!(decode_text(oid::BOOL, "t"), Value::Bool(true));
        assert_eq!(decode_text(oid::BOOL, "f"), Value::Bool(false));
        assert_eq!(decode_text(oid::INT4, "42"), Value::UInt(42));
        assert_eq!(decode_text(oid::INT8, "-7"), Value::Int(-7));
        assert_eq!(decode_text(oid::INT2, "x"), Value::String("x".into()));
        assert_eq!(
            decode_text(oid::OID, "4294967295"),
            Value::UInt(4_294_967_295)
        );
        assert_eq!(decode_text(oid::OID, "-1"), Value::String("-1".into()));
        assert_eq!(decode_text(oid::FLOAT8, "1.5"), Value::Float(1.5));
        assert_eq!(decode_text(oid::FLOAT4, "NaN"), Value::String("NaN".into()));
        assert_eq!(
            decode_text(oid::FLOAT8, "Infinity"),
            Value::String("Infinity".into())
        );
        assert_eq!(decode_text(oid::NUMERIC, "12"), Value::UInt(12));
        assert_eq!(decode_text(oid::NUMERIC, "12.50"), Value::Float(12.5));
        assert_eq!(
            decode_text(oid::NUMERIC, "NaN"),
            Value::String("NaN".into())
        );
        assert_eq!(
            decode_text(oid::TEXT, "hello"),
            Value::String("hello".into())
        );
        assert_eq!(
            decode_text(oid::UUID, "6f1c0d9e-0000-4000-8000-000000000000"),
            Value::String("6f1c0d9e-0000-4000-8000-000000000000".into())
        );
        assert_eq!(
            decode_text(9999, "whatever"),
            Value::String("whatever".into())
        );
        assert_eq!(
            decode_text(oid::DATE, "2026-10-23"),
            Value::String("2026-10-23".into())
        );
    }

    #[quench_test::test]
    fn json_columns_are_parsed() {
        assert_eq!(
            decode_text(oid::JSONB, r#"{"a": [1, 2]}"#),
            json!({"a": [1, 2]})
        );
        assert_eq!(decode_text(oid::JSON, "[true]"), json!([true]));
        assert_eq!(
            decode_text(oid::JSONB, "{broken"),
            Value::String("{broken".into())
        );
    }

    #[quench_test::test]
    fn bytea_round_trips() {
        assert_eq!(decode_text(oid::BYTEA, "\\x00ff10"), json!([0, 255, 16]));
        assert_eq!(
            decode_text(oid::BYTEA, "plain"),
            Value::String("plain".into())
        );
        assert_eq!(decode_bytea("\\x48656c6c6f").unwrap(), b"Hello");
        assert!(decode_bytea("\\xzz").is_none());
        assert_eq!(encode_bytea(b"Hello"), "\\x48656c6c6f");
        assert_eq!(encode_bytea(b""), "\\x");
        assert_eq!(decode_bytea("\\x").unwrap(), b"");
    }

    #[quench_test::test]
    fn timestamps_are_normalised_to_utc() {
        let cases = [
            ("2026-10-23 01:38:00+00", "2026-10-23T01:38:00Z"),
            (
                "2026-10-23 01:38:00.123456+00",
                "2026-10-23T01:38:00.123456Z",
            ),
            ("2026-10-23 03:38:00+02", "2026-10-23T01:38:00Z"),
            ("2026-10-23 01:38:00-05:30", "2026-10-23T07:08:00Z"),
            ("2026-10-23 01:38:00.5+01:00:30", "2026-10-23T00:37:30.500Z"),
            ("1999-12-31 23:59:59+00", "1999-12-31T23:59:59Z"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                decode_text(oid::TIMESTAMPTZ, input),
                Value::String(expected.into()),
                "{input}"
            );
        }
        assert_eq!(
            decode_text(oid::TIMESTAMP, "2026-10-23 01:38:00.25"),
            Value::String("2026-10-23T01:38:00.250Z".into())
        );
        for odd in [
            "infinity",
            "-infinity",
            "0044-03-15 12:00:00+00 BC",
            "garbage",
            "2026-10-23",
            "2026-13-40 00:00:00+00",
            "2026-10-23 25:00:00+00",
            "2026-10-23 01:38+00",
            "2026-10-23 01:38:00.1234567891+00",
        ] {
            assert_eq!(
                decode_text(oid::TIMESTAMPTZ, odd),
                Value::String(odd.into()),
                "{odd}"
            );
        }
        // A timestamptz without an offset is malformed; a naive timestamp with one is read leniently.
        assert!(parse_timestamp("2026-10-23 01:38:00", false).is_none());
        assert!(parse_timestamp("2026-10-23 01:38:00", true).is_some());
        assert!(parse_timestamp("2026-10-23-01 01:38:00+00", false).is_none());
        assert!(parse_timestamp("2026-10-23 01:38:00:01+00", false).is_none());
    }

    #[quench_test::test]
    fn arrays_parse() {
        assert_eq!(decode_text(1007, "{1,2,3}"), json!([1, 2, 3]));
        assert_eq!(
            decode_text(1009, r#"{a,"b c","say \"hi\"",NULL,"NULL"}"#),
            json!(["a", "b c", "say \"hi\"", null, "NULL"])
        );
        assert_eq!(decode_text(1007, "{}"), json!([]));
        assert_eq!(decode_text(1007, "{{1,2},{3,4}}"), json!([[1, 2], [3, 4]]));
        assert_eq!(decode_text(1007, "[0:2]={7,8,9}"), json!([7, 8, 9]));
        assert_eq!(decode_text(1000, "{t,f}"), json!([true, false]));
        assert_eq!(
            decode_text(1185, "{\"2026-10-23 03:38:00+02\"}"),
            json!(["2026-10-23T01:38:00Z"])
        );
        assert_eq!(decode_text(3807, r#"{"{\"a\": 1}"}"#), json!([{"a": 1}]));
        assert_eq!(decode_text(1009, " { a , b } ".trim()), json!(["a", "b"]));
        for bad in ["{1,2", "1,2}", "{\"unterminated}", "{1,2}x", "{\"a\\"] {
            assert_eq!(decode_text(1007, bad), Value::String(bad.into()), "{bad}");
        }
        for oid in [
            1000, 1001, 1002, 1003, 1005, 1014, 1015, 1016, 1021, 1022, 1028, 1115, 1182, 1183,
            1231, 199, 2951, 3807,
        ] {
            assert!(array_element(oid).is_some(), "{oid}");
        }
        assert!(array_element(12345).is_none());
    }

    #[quench_test::test]
    fn parameters_encode_as_text() {
        assert_eq!("x".to_sql(), Some("x".into()));
        assert_eq!(String::from("y").to_sql(), Some("y".into()));
        assert_eq!(true.to_sql(), Some("t".into()));
        assert_eq!(false.to_sql(), Some("f".into()));
        assert_eq!((-5i32).to_sql(), Some("-5".into()));
        assert_eq!(7u64.to_sql(), Some("7".into()));
        assert_eq!(1.5f64.to_sql(), Some("1.5".into()));
        assert_eq!(f64::NAN.to_sql(), Some("NaN".into()));
        assert_eq!(f64::INFINITY.to_sql(), Some("Infinity".into()));
        assert_eq!(f64::NEG_INFINITY.to_sql(), Some("-Infinity".into()));
        assert_eq!(0.1f32.to_sql(), Some("0.1".into()));
        assert_eq!(f32::NAN.to_sql(), Some("NaN".into()));
        assert_eq!(
            Time::from_unix(0).to_sql(),
            Some("1970-01-01T00:00:00Z".into())
        );
        assert_eq!(
            Uuid::NIL.to_sql(),
            Some("00000000-0000-0000-0000-000000000000".into())
        );
        assert_eq!(json!({"a": 1}).to_sql(), Some(r#"{"a":1}"#.into()));
        assert_eq!(vec![1u8, 255].to_sql(), Some("\\x01ff".into()));
        assert_eq!(b"hi"[..].to_sql(), Some("\\x6869".into()));
        assert_eq!(Some(3i16).to_sql(), Some("3".into()));
        assert_eq!(None::<i16>.to_sql(), None);
        assert_eq!((&&5i64).to_sql(), Some("5".into()));
        assert_eq!(
            Box::new(String::from("boxed")).to_sql(),
            Some("boxed".into())
        );
        assert_eq!(
            PgArray(vec!["a", "b \"c\"", "d\\e"]).to_sql(),
            Some(r#"{"a","b \"c\"","d\\e"}"#.into())
        );
        assert_eq!(
            PgArray(vec![Some(1), None]).to_sql(),
            Some(r#"{"1",NULL}"#.into())
        );
        assert_eq!(PgArray::<i32>(vec![]).to_sql(), Some("{}".into()));
    }

    #[quench_test::test]
    fn typed_reads() {
        assert_eq!(String::from_sql(&col(oid::TEXT), Some("s")).unwrap(), "s");
        assert!(bool::from_sql(&col(oid::BOOL), Some("t")).unwrap());
        assert!(!bool::from_sql(&col(oid::BOOL), Some("false")).unwrap());
        assert!(bool::from_sql(&col(oid::BOOL), Some("maybe")).is_err());
        assert_eq!(i16::from_sql(&col(oid::INT2), Some("-3")).unwrap(), -3);
        assert_eq!(
            i32::from_sql(&col(oid::INT4), Some("70000")).unwrap(),
            70_000
        );
        assert_eq!(
            i64::from_sql(&col(oid::INT8), Some("9000000000")).unwrap(),
            9_000_000_000
        );
        assert_eq!(u32::from_sql(&col(oid::OID), Some("5")).unwrap(), 5);
        assert_eq!(u64::from_sql(&col(oid::INT8), Some("5")).unwrap(), 5);
        assert!(
            (f64::from_sql(&col(oid::FLOAT8), Some("2.5")).unwrap() - 2.5).abs() < f64::EPSILON
        );
        assert!(
            (f32::from_sql(&col(oid::FLOAT4), Some("0.5")).unwrap() - 0.5).abs() < f32::EPSILON
        );
        assert!(i32::from_sql(&col(oid::INT4), Some("many")).is_err());
        assert_eq!(
            Time::from_sql(&col(oid::TIMESTAMPTZ), Some("2026-10-23 01:38:00+00"))
                .unwrap()
                .timestamp(),
            1_792_719_480
        );
        assert!(Time::from_sql(&col(oid::TIMESTAMPTZ), Some("infinity")).is_err());
        assert_eq!(
            Time::from_sql(&col(oid::TIMESTAMP), Some("2026-10-23 01:38:00"))
                .unwrap()
                .timestamp(),
            1_792_719_480
        );
        assert_eq!(
            Uuid::from_sql(
                &col(oid::UUID),
                Some("00000000-0000-0000-0000-000000000000")
            )
            .unwrap(),
            Uuid::NIL
        );
        assert!(Uuid::from_sql(&col(oid::UUID), Some("nope")).is_err());
        assert_eq!(
            Vec::<u8>::from_sql(&col(oid::BYTEA), Some("\\x0102")).unwrap(),
            [1, 2]
        );
        assert!(Vec::<u8>::from_sql(&col(oid::BYTEA), Some("0102")).is_err());
        assert_eq!(
            Value::from_sql(&col(oid::JSONB), Some("[1]")).unwrap(),
            json!([1])
        );
        assert_eq!(
            Value::from_sql(&col(oid::JSONB), None).unwrap(),
            Value::Null
        );
        assert_eq!(
            Option::<i32>::from_sql(&col(oid::INT4), None).unwrap(),
            None
        );
        assert_eq!(
            Option::<i32>::from_sql(&col(oid::INT4), Some("4")).unwrap(),
            Some(4)
        );
        assert!(Option::<i32>::from_sql(&col(oid::INT4), Some("x")).is_err());
        let err = String::from_sql(&col(oid::TEXT), None).unwrap_err();
        assert!(err.to_string().contains("NULL"), "{err}");
    }
}
