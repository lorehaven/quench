//! Operations on template values: truthiness, comparison, arithmetic, and how values print.

use crate::error::{Error, ErrorKind, Result};
use quench_serde::{Value, json};
use std::cmp::Ordering;
use std::fmt::Write as _;

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::UInt(n) => *n != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => o.iter().len() > 0,
    }
}

/// A number, widened so integer arithmetic cannot overflow before it is range-checked.
#[derive(Clone, Copy)]
pub enum Num {
    Int(i128),
    Float(f64),
}

pub fn num(value: &Value) -> Option<Num> {
    match value {
        Value::Int(n) => Some(Num::Int(i128::from(*n))),
        Value::UInt(n) => Some(Num::Int(i128::from(*n))),
        Value::Float(f) => Some(Num::Float(*f)),
        Value::Bool(b) => Some(Num::Int(i128::from(*b))),
        _ => None,
    }
}

const fn as_f64(n: Num) -> f64 {
    match n {
        Num::Int(i) => i as f64,
        Num::Float(f) => f,
    }
}

pub fn from_int(n: i128) -> Result<Value> {
    if let Ok(n) = i64::try_from(n) {
        Ok(Value::from(n))
    } else if let Ok(n) = u64::try_from(n) {
        Ok(Value::UInt(n))
    } else {
        Err(Error::new(ErrorKind::InvalidOperation, "integer overflow"))
    }
}

pub fn eq(a: &Value, b: &Value) -> bool {
    match (num(a), num(b)) {
        (Some(x), Some(y)) if !matches!((a, b), (Value::Bool(_), Value::Bool(_))) => {
            matches!(cmp_num(x, y), Some(Ordering::Equal))
        }
        _ => match (a, b) {
            (Value::Array(x), Value::Array(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(p, q)| eq(p, q))
            }
            (Value::Object(x), Value::Object(y)) => {
                x.iter().len() == y.iter().len()
                    && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| eq(v, w)))
            }
            _ => a == b,
        },
    }
}

fn cmp_num(a: Num, b: Num) -> Option<Ordering> {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => Some(x.cmp(&y)),
        _ => as_f64(a).partial_cmp(&as_f64(b)),
    }
}

pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    if let (Some(x), Some(y)) = (num(a), num(b)) {
        return cmp_num(x, y);
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Array(x), Value::Array(y)) => {
            for (p, q) in x.iter().zip(y) {
                match compare(p, q)? {
                    Ordering::Equal => {}
                    other => return Some(other),
                }
            }
            Some(x.len().cmp(&y.len()))
        }
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        _ => None,
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidOperation, message)
}

pub const fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "none",
        Value::Bool(_) => "bool",
        Value::Int(_) | Value::UInt(_) | Value::Float(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "sequence",
        Value::Object(_) => "map",
    }
}

pub fn add(a: &Value, b: &Value) -> Result<Value> {
    match (a, b) {
        (Value::String(x), Value::String(y)) => Ok(Value::String(format!("{x}{y}"))),
        (Value::Array(x), Value::Array(y)) => {
            Ok(Value::Array(x.iter().chain(y).cloned().collect()))
        }
        _ => arithmetic('+', a, b),
    }
}

pub fn arithmetic(op: char, a: &Value, b: &Value) -> Result<Value> {
    // `*` also repeats strings and lists.
    if op == '*' {
        let repeat = |v: &Value, n: &Value| -> Option<Value> {
            let Some(Num::Int(n)) = num(n) else {
                return None;
            };
            let n = usize::try_from(n.max(0)).ok()?;
            match v {
                Value::String(s) => Some(Value::String(s.repeat(n))),
                Value::Array(a) => Some(Value::Array(
                    a.iter().cloned().cycle().take(a.len() * n).collect(),
                )),
                _ => None,
            }
        };
        if let Some(v) = repeat(a, b).or_else(|| repeat(b, a)) {
            return Ok(v);
        }
    }
    let (Some(x), Some(y)) = (num(a), num(b)) else {
        return Err(invalid(format!(
            "cannot apply `{op}` to {} and {}",
            type_name(a),
            type_name(b)
        )));
    };
    match (op, x, y) {
        ('+', Num::Int(p), Num::Int(q)) => from_int(p + q),
        ('-', Num::Int(p), Num::Int(q)) => from_int(p - q),
        ('*', Num::Int(p), Num::Int(q)) => from_int(
            p.checked_mul(q)
                .ok_or_else(|| invalid("integer overflow".into()))?,
        ),
        ('/', _, _) => {
            let d = as_f64(y);
            if d == 0.0 {
                return Err(invalid("division by zero".into()));
            }
            Ok(Value::Float(as_f64(x) / d))
        }
        ('d', Num::Int(p), Num::Int(q)) => {
            if q == 0 {
                return Err(invalid("division by zero".into()));
            }
            from_int(p.div_euclid(q) - i128::from(q < 0 && p.rem_euclid(q) != 0))
        }
        ('d', _, _) => {
            let d = as_f64(y);
            if d == 0.0 {
                return Err(invalid("division by zero".into()));
            }
            Ok(Value::Float((as_f64(x) / d).floor()))
        }
        ('%', Num::Int(p), Num::Int(q)) => {
            if q == 0 {
                return Err(invalid("division by zero".into()));
            }
            let r = p % q;
            from_int(if r != 0 && (r < 0) != (q < 0) {
                r + q
            } else {
                r
            })
        }
        ('%', _, _) => {
            let d = as_f64(y);
            let r = as_f64(x) % d;
            Ok(Value::Float(if r != 0.0 && (r < 0.0) != (d < 0.0) {
                r + d
            } else {
                r
            }))
        }
        ('p', Num::Int(p), Num::Int(q)) if q >= 0 => {
            let exponent = u32::try_from(q).map_err(|_| invalid("exponent too large".into()))?;
            from_int(
                p.checked_pow(exponent)
                    .ok_or_else(|| invalid("integer overflow".into()))?,
            )
        }
        ('p', _, _) => Ok(Value::Float(as_f64(x).powf(as_f64(y)))),
        ('+', _, _) => Ok(Value::Float(as_f64(x) + as_f64(y))),
        ('-', _, _) => Ok(Value::Float(as_f64(x) - as_f64(y))),
        ('*', _, _) => Ok(Value::Float(as_f64(x) * as_f64(y))),
        _ => Err(invalid(format!("unknown operator `{op}`"))),
    }
}

pub fn contains(container: &Value, item: &Value) -> Result<bool> {
    match container {
        Value::String(s) => match item {
            Value::String(needle) => Ok(s.contains(needle.as_str())),
            _ => Ok(false),
        },
        Value::Array(items) => Ok(items.iter().any(|v| eq(v, item))),
        Value::Object(map) => Ok(matches!(item, Value::String(k) if map.contains_key(k))),
        other => Err(invalid(format!(
            "cannot look for a value in a {}",
            type_name(other)
        ))),
    }
}

/// How a float prints: always with a fractional part, like Python.
fn float_text(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        f.to_string()
    }
}

/// Prints a value as a template shows it: strings plain, everything else in a literal-like form.
pub fn display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => repr(other),
    }
}

pub fn repr(value: &Value) -> String {
    match value {
        Value::Null => "none".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Int(n) => n.to_string(),
        Value::UInt(n) => n.to_string(),
        Value::Float(f) => float_text(*f),
        Value::String(s) => json::to_string(&Value::String(s.clone())),
        Value::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&repr(item));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut out = String::from("{");
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(
                    out,
                    "{}: {}",
                    json::to_string(&Value::String(k.clone())),
                    repr(v)
                );
            }
            out.push('}');
            out
        }
    }
}
