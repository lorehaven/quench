//! Typing plain scalars: the YAML 1.2 core schema.

use crate::Value;

/// What an unquoted, untagged scalar means.
pub fn plain(text: &str) -> Value {
    match text {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => {
            return Value::Float(f64::INFINITY);
        }
        "-.inf" | "-.Inf" | "-.INF" => return Value::Float(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => return Value::Float(f64::NAN),
        _ => {}
    }
    integer(text)
        .or_else(|| float(text))
        .unwrap_or_else(|| Value::String(text.to_string()))
}

fn integer(text: &str) -> Option<Value> {
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return u64::from_str_radix(digits, radix)
                .ok()
                .filter(|_| digits.bytes().all(|b| b != b'+'))
                .map(Value::UInt);
        }
    }
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // `007` is text, not octal: a leading zero keeps it a string.
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    if negative {
        return Some(match format!("-{digits}").parse::<i64>() {
            Ok(0) => Value::UInt(0),
            Ok(n) => Value::Int(n),
            Err(_) => Value::Float(-digits.parse::<f64>().ok()?),
        });
    }
    // Beyond 64 bits an integer is read as a float.
    digits.parse::<u64>().map_or_else(
        |_| digits.parse::<f64>().ok().map(Value::Float),
        |n| Some(Value::UInt(n)),
    )
}

fn float(text: &str) -> Option<Value> {
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(m, e)| (m, Some(e)));
    let (whole, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(w, f)| (w, Some(f)));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole)
        || fraction.is_some_and(|f| !digits(f))
        || (whole.is_empty() && fraction.is_none_or(str::is_empty))
    {
        return None;
    }
    if let Some(e) = exponent {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        if e.is_empty() || !digits(e) {
            return None;
        }
    }
    // A bare integer was handled above; without `.` or an exponent this is not a float.
    if fraction.is_none() && exponent.is_none() {
        return None;
    }
    text.parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .map(Value::Float)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn nulls_and_booleans() {
        for text in ["", "~", "null", "Null", "NULL"] {
            assert_eq!(plain(text), Value::Null, "{text:?}");
        }
        for (text, want) in [
            ("true", true),
            ("True", true),
            ("TRUE", true),
            ("false", false),
            ("False", false),
            ("FALSE", false),
        ] {
            assert_eq!(plain(text), Value::Bool(want));
        }
        // YAML 1.1 spellings are plain text in 1.2
        for text in ["yes", "no", "on", "off", "y", "n", "tRUE", "nULL"] {
            assert_eq!(plain(text), Value::from(text), "{text}");
        }
    }

    #[quench_test::test]
    fn integers() {
        assert_eq!(plain("0"), Value::UInt(0));
        assert_eq!(plain("-0"), Value::UInt(0));
        assert_eq!(plain("42"), Value::UInt(42));
        assert_eq!(plain("-42"), Value::Int(-42));
        assert_eq!(plain("+42"), Value::UInt(42));
        assert_eq!(plain("0x1F"), Value::UInt(31));
        assert_eq!(plain("0o17"), Value::UInt(15));
        assert_eq!(plain("0b11"), Value::UInt(3));
        assert_eq!(plain("18446744073709551615"), Value::UInt(u64::MAX));
        assert_eq!(plain("-9223372036854775808"), Value::Int(i64::MIN));
        assert_eq!(
            plain("18446744073709551616"),
            Value::Float(18_446_744_073_709_551_616.0)
        );
        for text in [
            "007", "-007", "1_000", "0x", "0xZZ", "0o9", "0b2", "1-2", "--1", "+", "-", "0x+1",
            "12a",
        ] {
            assert!(matches!(plain(text), Value::String(_)), "{text}");
        }
    }

    #[quench_test::test]
    fn floats() {
        assert_eq!(plain("1.5"), Value::Float(1.5));
        assert_eq!(plain("-1.5"), Value::Float(-1.5));
        assert_eq!(plain(".5"), Value::Float(0.5));
        assert_eq!(plain("+.5"), Value::Float(0.5));
        assert_eq!(plain("5."), Value::Float(5.0));
        assert_eq!(plain("1e3"), Value::Float(1000.0));
        assert_eq!(plain("1E-3"), Value::Float(0.001));
        assert_eq!(plain("1.5e+2"), Value::Float(150.0));
        assert_eq!(plain("0.0"), Value::Float(0.0));
        assert_eq!(plain(".inf"), Value::Float(f64::INFINITY));
        assert_eq!(plain("-.INF"), Value::Float(f64::NEG_INFINITY));
        assert!(matches!(plain(".NaN"), Value::Float(f) if f.is_nan()));
        for text in [
            ".",
            "e5",
            "1e",
            "1e+",
            "1.2.3",
            "0.1.2",
            ".e1",
            "1.5x",
            "-.",
            "+.e2",
            "1e5e5",
            "1.5e1.5",
            "1,5",
            "1:2",
            "2001-01-23",
            "1e999",
        ] {
            assert!(matches!(plain(text), Value::String(_)), "{text}");
        }
    }

    #[quench_test::test]
    fn everything_else_is_text() {
        for text in [
            "hello", "http://x", "a:b", "=", "<<", "@x", "a b c", "-x", "x-", "1 2",
        ] {
            assert_eq!(plain(text), Value::from(text));
        }
    }
}
