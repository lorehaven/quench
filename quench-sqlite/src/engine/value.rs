//! SQLite's rules for values: type affinity, conversions between storage classes, how a real prints, and
//! three-valued truth.

use std::cmp::Ordering;

use crate::record::Value;

/// The affinity a column's declared type gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity {
    /// No affinity (`BLOB` or no type): values are stored as given.
    Blob,
    Text,
    Numeric,
    Integer,
    Real,
}

/// The affinity of a declared type, by SQLite's five rules in their order.
#[must_use]
pub fn affinity_of(type_name: Option<&str>) -> Affinity {
    let Some(name) = type_name else {
        return Affinity::Blob;
    };
    let t = name.to_ascii_uppercase();
    if t.contains("INT") {
        Affinity::Integer
    } else if t.contains("CHAR") || t.contains("CLOB") || t.contains("TEXT") {
        Affinity::Text
    } else if t.contains("BLOB") || t.is_empty() {
        Affinity::Blob
    } else if t.contains("REAL") || t.contains("FLOA") || t.contains("DOUB") {
        Affinity::Real
    } else {
        Affinity::Numeric
    }
}

/// How a collation orders text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Collation {
    #[default]
    Binary,
    NoCase,
    RTrim,
}

impl Collation {
    /// By name, case blind.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "BINARY" => Some(Self::Binary),
            "NOCASE" => Some(Self::NoCase),
            "RTRIM" => Some(Self::RTrim),
            _ => None,
        }
    }

    /// Compares two texts.
    #[must_use]
    pub fn compare(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Binary => a.as_bytes().cmp(b.as_bytes()),
            Self::NoCase => a
                .bytes()
                .map(|c| c.to_ascii_lowercase())
                .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
            Self::RTrim => a
                .trim_end_matches(' ')
                .as_bytes()
                .cmp(b.trim_end_matches(' ').as_bytes()),
        }
    }
}

/// A real as SQLite prints it (`%!.15g`): 15 significant digits, always a point or an exponent.
#[must_use]
pub fn format_real(x: f64) -> String {
    if x.is_nan() {
        return String::new();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Inf".into() } else { "-Inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // 15 significant digits when they read back as the same number, else 17 (as SQLite 3.4x does).
    let short = format!("{x:.14e}");
    let sci = if short.parse::<f64>().is_ok_and(|back| back == x) {
        short
    } else {
        format!("{x:.16e}")
    };
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if negative { "-" } else { "" };
    if !(-4..17).contains(&exp) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        let esign = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{first}.{rest}e{esign}{:02}", exp.abs());
    }
    if exp >= 0 {
        let int_len = exp as usize + 1;
        let (int_part, frac) = if digits.len() <= int_len {
            (
                format!("{digits}{}", "0".repeat(int_len - digits.len())),
                String::new(),
            )
        } else {
            (digits[..int_len].to_string(), digits[int_len..].to_string())
        };
        let frac = if frac.is_empty() {
            "0".to_string()
        } else {
            frac
        };
        format!("{sign}{int_part}.{frac}")
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

/// The text form of a value (what `||`, `CAST AS TEXT` and text affinity give).
#[must_use]
pub fn to_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::Integer(i) => Some(i.to_string()),
        Value::Real(r) => Some(format_real(*r)),
        Value::Text(s) => Some(s.clone()),
        Value::Blob(b) => Some(String::from_utf8_lossy(b).into_owned()),
    }
}

/// The whole text as a number, if it is one (surrounding blanks allowed): an integer when it is written as one
/// and fits, otherwise a real.
#[must_use]
pub fn parse_number(text: &str) -> Option<Value> {
    let t = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let (value, used) = number_prefix(t)?;
    (used == t.len()).then_some(value)
}

/// The longest numeric prefix of the text and its length, `None` when it does not start with a number.
#[must_use]
pub fn number_prefix(text: &str) -> Option<(Value, usize)> {
    let b = text.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i += 1;
    }
    let digits_start = i;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let int_digits = i - digits_start;
    let mut real = false;
    let mut frac_digits = 0;
    if b.get(i) == Some(&b'.') {
        let mut j = i + 1;
        while b.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
        }
        frac_digits = j - i - 1;
        if int_digits > 0 || frac_digits > 0 {
            real = true;
            i = j;
        }
    }
    if int_digits == 0 && frac_digits == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            real = true;
            i = j;
        }
    }
    let slice = &text[..i];
    if !real && let Ok(n) = slice.parse::<i64>() {
        return Some((Value::Integer(n), i));
    }
    slice.parse::<f64>().ok().map(|r| (Value::Real(r), i))
}

/// Applies a column's affinity to a value about to be stored (or compared).
#[must_use]
pub fn apply_affinity(v: Value, affinity: Affinity) -> Value {
    match (affinity, v) {
        (Affinity::Text, Value::Integer(i)) => Value::Text(i.to_string()),
        (Affinity::Text, Value::Real(r)) => Value::Text(format_real(r)),
        (Affinity::Integer | Affinity::Numeric, Value::Text(s)) => match parse_number(&s) {
            Some(Value::Real(r)) => real_to_integer_if_exact(r),
            Some(n) => n,
            None => Value::Text(s),
        },
        (Affinity::Integer | Affinity::Numeric, Value::Real(r)) => real_to_integer_if_exact(r),
        (Affinity::Real, Value::Integer(i)) => Value::Real(i as f64),
        (Affinity::Real, Value::Text(s)) => match parse_number(&s) {
            Some(Value::Integer(i)) => Value::Real(i as f64),
            Some(n) => n,
            None => Value::Text(s),
        },
        (_, v) => v,
    }
}

fn real_to_integer_if_exact(r: f64) -> Value {
    if r.fract() == 0.0 && (-9.223_372_036_854_775_8e18..9.223_372_036_854_775_8e18).contains(&r) {
        Value::Integer(r as i64)
    } else {
        Value::Real(r)
    }
}

/// A value as a number for arithmetic: texts count by their numeric prefix (0 when there is none), blobs as
/// their text, NULL stays NULL.
#[must_use]
pub fn to_number(v: &Value) -> Option<Value> {
    match v {
        Value::Null => None,
        Value::Integer(_) | Value::Real(_) => Some(v.clone()),
        Value::Text(s) => Some(number_prefix(s.trim_start()).map_or(Value::Integer(0), |(n, _)| n)),
        Value::Blob(b) => Some(
            number_prefix(String::from_utf8_lossy(b).trim_start())
                .map_or(Value::Integer(0), |(n, _)| n),
        ),
    }
}

/// Truth of a value: `None` for NULL.
#[must_use]
pub fn truth(v: &Value) -> Option<bool> {
    match to_number(v)? {
        Value::Integer(i) => Some(i != 0),
        Value::Real(r) => Some(r != 0.0),
        _ => None,
    }
}

/// `CAST(v AS type)`.
#[must_use]
pub fn cast(v: Value, type_name: &str) -> Value {
    if matches!(v, Value::Null) {
        return Value::Null;
    }
    match affinity_of(Some(type_name)) {
        Affinity::Blob => match v {
            Value::Blob(_) => v,
            other => Value::Blob(to_text(&other).unwrap_or_default().into_bytes()),
        },
        Affinity::Text => Value::Text(to_text(&v).unwrap_or_default()),
        Affinity::Integer => match to_number(&v) {
            Some(Value::Integer(i)) => Value::Integer(i),
            Some(Value::Real(r)) => Value::Integer(real_to_i64(r)),
            _ => Value::Integer(0),
        },
        Affinity::Real => match to_number(&v) {
            Some(Value::Integer(i)) => Value::Real(i as f64),
            Some(r @ Value::Real(_)) => r,
            _ => Value::Real(0.0),
        },
        Affinity::Numeric => match to_number(&v) {
            Some(Value::Real(r)) => real_to_integer_if_exact(r),
            Some(n) => n,
            None => Value::Integer(0),
        },
    }
}

/// A real as an integer the way SQLite casts: truncation, saturating at the ends.
#[must_use]
pub fn real_to_i64(r: f64) -> i64 {
    if r.is_nan() {
        0
    } else if r >= 9.223_372_036_854_775_8e18 {
        i64::MAX
    } else if r <= -9.223_372_036_854_775_8e18 {
        i64::MIN
    } else {
        r as i64
    }
}

/// The name of a value's storage class, as `typeof()` gives it.
#[must_use]
pub const fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Integer(_) => "integer",
        Value::Real(_) => "real",
        Value::Text(_) => "text",
        Value::Blob(_) => "blob",
    }
}

/// Orders two non-NULL values: numbers by value, then text (by the collation), then blobs.
#[must_use]
pub fn compare(a: &Value, b: &Value, collation: Collation) -> Ordering {
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => collation.compare(x, y),
        _ => crate::tree::compare_values(a, b),
    }
}
