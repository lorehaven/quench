//! Operators and functions on values, with SQLite's NULL logic: arithmetic, comparison, `LIKE`/`GLOB`, the
//! scalar functions, and the aggregates.

use std::cmp::Ordering;

use super::value::{
    Affinity, Collation, apply_affinity, cast, compare, format_real, real_to_i64, to_number,
    to_text, truth, type_name,
};
use crate::error::{Error, Result};
use crate::record::Value;
use crate::sql::ast::BinaryOp;

fn bool_value(b: bool) -> Value {
    Value::Integer(i64::from(b))
}

/// Arithmetic, bit operations and `||`. NULL in gives NULL out; integer overflow becomes a real; division by
/// zero is NULL.
#[must_use]
pub fn arithmetic(op: BinaryOp, a: &Value, b: &Value) -> Value {
    if op == BinaryOp::Concat {
        return match (to_text(a), to_text(b)) {
            (Some(x), Some(y)) => Value::Text(x + &y),
            _ => Value::Null,
        };
    }
    let (Some(x), Some(y)) = (to_number(a), to_number(b)) else {
        return Value::Null;
    };
    match op {
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            let (x, y) = (as_i64(&x), as_i64(&y));
            Value::Integer(match op {
                BinaryOp::BitAnd => x & y,
                BinaryOp::BitOr => x | y,
                BinaryOp::ShiftLeft => shift(x, y),
                _ => shift(x, y.checked_neg().unwrap_or(i64::MAX)),
            })
        }
        BinaryOp::Rem => {
            let (x, y) = (as_i64(&x), as_i64(&y));
            if y == 0 {
                Value::Null
            } else {
                Value::Integer(x.checked_rem(y).unwrap_or(0))
            }
        }
        _ => match (&x, &y) {
            (Value::Integer(i), Value::Integer(j)) => integer_arithmetic(op, *i, *j),
            _ => real_arithmetic(op, as_f64(&x), as_f64(&y)),
        },
    }
}

fn integer_arithmetic(op: BinaryOp, i: i64, j: i64) -> Value {
    let exact = match op {
        BinaryOp::Add => i.checked_add(j),
        BinaryOp::Sub => i.checked_sub(j),
        BinaryOp::Mul => i.checked_mul(j),
        BinaryOp::Div => {
            if j == 0 {
                return Value::Null;
            }
            i.checked_div(j)
        }
        _ => None,
    };
    exact.map_or_else(|| real_arithmetic(op, i as f64, j as f64), Value::Integer)
}

fn real_arithmetic(op: BinaryOp, x: f64, y: f64) -> Value {
    let r = match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => {
            if y == 0.0 {
                return Value::Null;
            }
            x / y
        }
        _ => return Value::Null,
    };
    if r.is_nan() {
        Value::Null
    } else {
        Value::Real(r)
    }
}

fn as_i64(v: &Value) -> i64 {
    match v {
        Value::Integer(i) => *i,
        Value::Real(r) => real_to_i64(*r),
        _ => 0,
    }
}

const fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Integer(i) => *i as f64,
        Value::Real(r) => *r,
        _ => 0.0,
    }
}

const fn shift(x: i64, by: i64) -> i64 {
    if by >= 64 {
        0
    } else if by >= 0 {
        x.wrapping_shl(by as u32)
    } else if by <= -64 {
        if x < 0 { -1 } else { 0 }
    } else {
        x >> (-by)
    }
}

/// What a comparison knows about its operands: their affinities and the collation to use.
#[derive(Debug, Clone, Copy)]
pub struct CompareRules {
    pub left: Affinity,
    pub right: Affinity,
    pub collation: Collation,
}

/// Applies the comparison affinity rules to a pair: a numeric side makes the other numeric if it is text; a
/// text side makes the other text when that has no affinity.
#[must_use]
pub fn coerce_pair(a: &Value, b: &Value, rules: CompareRules) -> (Value, Value) {
    let numeric = |x: Affinity| matches!(x, Affinity::Integer | Affinity::Real | Affinity::Numeric);
    let (mut x, mut y) = (a.clone(), b.clone());
    if numeric(rules.left) && !numeric(rules.right) && matches!(y, Value::Text(_) | Value::Blob(_))
    {
        if matches!(y, Value::Text(_)) {
            y = apply_affinity(y, Affinity::Numeric);
        }
    } else if numeric(rules.right)
        && !numeric(rules.left)
        && matches!(x, Value::Text(_) | Value::Blob(_))
    {
        if matches!(x, Value::Text(_)) {
            x = apply_affinity(x, Affinity::Numeric);
        }
    } else if rules.left == Affinity::Text && rules.right == Affinity::Blob {
        y = apply_affinity(y, Affinity::Text);
    } else if rules.right == Affinity::Text && rules.left == Affinity::Blob {
        x = apply_affinity(x, Affinity::Text);
    }
    (x, y)
}

/// A comparison operator (`=`, `<`, `IS`, ...) under three-valued logic.
#[must_use]
pub fn comparison(op: BinaryOp, a: &Value, b: &Value, rules: CompareRules) -> Value {
    if matches!(op, BinaryOp::Is | BinaryOp::IsNot) {
        let same = match (a, b) {
            (Value::Null, Value::Null) => true,
            (Value::Null, _) | (_, Value::Null) => false,
            _ => {
                let (x, y) = coerce_pair(a, b, rules);
                compare(&x, &y, rules.collation) == Ordering::Equal
            }
        };
        return bool_value(same == (op == BinaryOp::Is));
    }
    if matches!(a, Value::Null) || matches!(b, Value::Null) {
        return Value::Null;
    }
    let (x, y) = coerce_pair(a, b, rules);
    let o = compare(&x, &y, rules.collation);
    bool_value(match op {
        BinaryOp::Eq => o == Ordering::Equal,
        BinaryOp::Ne => o != Ordering::Equal,
        BinaryOp::Lt => o == Ordering::Less,
        BinaryOp::Le => o != Ordering::Greater,
        BinaryOp::Gt => o == Ordering::Greater,
        _ => o != Ordering::Less,
    })
}

/// `AND` and `OR` of two truth values, which may be unknown.
#[must_use]
pub fn logic(op: BinaryOp, a: Option<bool>, b: Option<bool>) -> Value {
    let r = if op == BinaryOp::And {
        match (a, b) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        }
    } else {
        match (a, b) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        }
    };
    r.map_or(Value::Null, bool_value)
}

/// `LIKE` (ASCII case blind, `%` and `_`) or `GLOB` (case sensitive, `*`, `?`, `[set]`).
#[must_use]
pub fn pattern_match(glob: bool, text: &str, pattern: &str, escape: Option<char>) -> bool {
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    if glob {
        glob_at(&t, &p)
    } else {
        like_at(&t, &p, escape)
    }
}

fn like_at(t: &[char], p: &[char], escape: Option<char>) -> bool {
    let Some((&first, rest)) = p.split_first() else {
        return t.is_empty();
    };
    if Some(first) == escape && !rest.is_empty() {
        return t.first().is_some_and(|c| c.eq_ignore_ascii_case(&rest[0]))
            && like_at(&t[1..], &rest[1..], escape);
    }
    match first {
        '%' => {
            let mut rest = rest;
            while rest.first() == Some(&'%') {
                rest = &rest[1..];
            }
            (0..=t.len()).any(|i| like_at(&t[i..], rest, escape))
        }
        '_' => !t.is_empty() && like_at(&t[1..], rest, escape),
        c => {
            t.first().is_some_and(|x| x.eq_ignore_ascii_case(&c)) && like_at(&t[1..], rest, escape)
        }
    }
}

fn glob_at(t: &[char], p: &[char]) -> bool {
    let Some((&first, rest)) = p.split_first() else {
        return t.is_empty();
    };
    match first {
        '*' => (0..=t.len()).any(|i| glob_at(&t[i..], rest)),
        '?' => !t.is_empty() && glob_at(&t[1..], rest),
        '[' => {
            let Some(&c) = t.first() else {
                return false;
            };
            let Some(close) = rest.iter().skip(1).position(|&x| x == ']').map(|i| i + 1) else {
                return false;
            };
            let set = &rest[..close];
            let (negate, set) = match set.split_first() {
                Some((&'^', s)) => (true, s),
                _ => (false, set),
            };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= (set[i]..=set[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != negate && glob_at(&t[1..], &rest[close + 1..])
        }
        c => t.first() == Some(&c) && glob_at(&t[1..], rest),
    }
}

fn arg_text(v: &Value) -> Option<String> {
    to_text(v)
}

/// The aggregate functions (they take rows, not values).
#[must_use]
pub fn is_aggregate(name: &str, args: usize) -> bool {
    match name {
        "count" | "sum" | "total" | "avg" | "group_concat" | "string_agg" => true,
        "min" | "max" => args == 1,
        _ => false,
    }
}

/// Calls a scalar function.
pub fn call_scalar(name: &str, args: &[Value]) -> Result<Value> {
    let arity = |n: usize| -> Result<()> {
        if args.len() == n {
            Ok(())
        } else {
            Err(Error::Unsupported(format!(
                "{name}() takes {n} argument(s), got {}",
                args.len()
            )))
        }
    };
    Ok(match name {
        "abs" => {
            arity(1)?;
            match to_number(&args[0]) {
                None => Value::Null,
                Some(Value::Integer(i)) => i
                    .checked_abs()
                    .map_or_else(|| Value::Real((i as f64).abs()), Value::Integer),
                Some(Value::Real(r)) => Value::Real(r.abs()),
                Some(_) => Value::Integer(0),
            }
        }
        "length" => {
            arity(1)?;
            match &args[0] {
                Value::Null => Value::Null,
                Value::Blob(b) => Value::Integer(b.len() as i64),
                v => Value::Integer(arg_text(v).unwrap_or_default().chars().count() as i64),
            }
        }
        "lower" | "upper" => {
            arity(1)?;
            arg_text(&args[0]).map_or(Value::Null, |s| {
                Value::Text(if name == "lower" {
                    s.to_ascii_lowercase()
                } else {
                    s.to_ascii_uppercase()
                })
            })
        }
        "typeof" => {
            arity(1)?;
            Value::Text(type_name(&args[0]).into())
        }
        "coalesce" | "ifnull" => {
            if args.len() < 2 {
                return Err(Error::Unsupported(format!(
                    "{name}() needs at least two arguments"
                )));
            }
            args.iter()
                .find(|v| !matches!(v, Value::Null))
                .cloned()
                .unwrap_or(Value::Null)
        }
        "nullif" => {
            arity(2)?;
            let rules = CompareRules {
                left: Affinity::Blob,
                right: Affinity::Blob,
                collation: Collation::Binary,
            };
            if truth(&comparison(BinaryOp::Eq, &args[0], &args[1], rules)) == Some(true) {
                Value::Null
            } else {
                args[0].clone()
            }
        }
        "iif" => {
            arity(3)?;
            if truth(&args[0]) == Some(true) {
                args[1].clone()
            } else {
                args[2].clone()
            }
        }
        "min" | "max" => {
            if args.len() < 2 {
                return Err(Error::Unsupported(format!(
                    "{name}() as a scalar needs two arguments"
                )));
            }
            if args.iter().any(|v| matches!(v, Value::Null)) {
                return Ok(Value::Null);
            }
            let mut best = args[0].clone();
            for v in &args[1..] {
                let o = compare(v, &best, Collation::Binary);
                if (name == "min" && o == Ordering::Less)
                    || (name == "max" && o == Ordering::Greater)
                {
                    best = v.clone();
                }
            }
            best
        }
        "substr" | "substring" => {
            if !(2..=3).contains(&args.len()) {
                return Err(Error::Unsupported("substr() takes 2 or 3 arguments".into()));
            }
            substr(&args[0], &args[1], args.get(2))
        }
        "trim" | "ltrim" | "rtrim" => {
            if !(1..=2).contains(&args.len()) {
                return Err(Error::Unsupported(format!(
                    "{name}() takes 1 or 2 arguments"
                )));
            }
            let Some(s) = arg_text(&args[0]) else {
                return Ok(Value::Null);
            };
            let set: Vec<char> = args
                .get(1)
                .and_then(arg_text)
                .unwrap_or_else(|| " ".into())
                .chars()
                .collect();
            let left = name != "rtrim";
            let right = name != "ltrim";
            let mut out = s.as_str();
            if left {
                out = out.trim_start_matches(|c| set.contains(&c));
            }
            if right {
                out = out.trim_end_matches(|c| set.contains(&c));
            }
            Value::Text(out.to_string())
        }
        "replace" => {
            arity(3)?;
            match (arg_text(&args[0]), arg_text(&args[1]), arg_text(&args[2])) {
                (Some(s), Some(from), Some(to)) => Value::Text(if from.is_empty() {
                    s
                } else {
                    s.replace(&from, &to)
                }),
                _ => Value::Null,
            }
        }
        "instr" => {
            arity(2)?;
            match (arg_text(&args[0]), arg_text(&args[1])) {
                (Some(h), Some(n)) => Value::Integer(
                    h.find(&n)
                        .map_or(0, |byte| h[..byte].chars().count() as i64 + 1),
                ),
                _ => Value::Null,
            }
        }
        "round" => {
            if !(1..=2).contains(&args.len()) {
                return Err(Error::Unsupported("round() takes 1 or 2 arguments".into()));
            }
            let digits = args
                .get(1)
                .map_or(0, |d| as_i64(&to_number(d).unwrap_or(Value::Integer(0))))
                .clamp(0, 30);
            match to_number(&args[0]) {
                None => Value::Null,
                Some(v) => {
                    let x = as_f64(&v);
                    let scale = 10_f64.powi(digits as i32);
                    Value::Real((x * scale).round() / scale)
                }
            }
        }
        "hex" => {
            arity(1)?;
            let bytes = match &args[0] {
                Value::Null => return Ok(Value::Text(String::new())),
                Value::Blob(b) => b.clone(),
                v => arg_text(v).unwrap_or_default().into_bytes(),
            };
            Value::Text(hex_upper(&bytes))
        }
        "quote" => {
            arity(1)?;
            Value::Text(match &args[0] {
                Value::Null => "NULL".into(),
                Value::Integer(i) => i.to_string(),
                Value::Real(r) => format_real(*r),
                Value::Text(s) => format!("'{}'", s.replace('\'', "''")),
                Value::Blob(b) => format!("X'{}'", hex_upper(b)),
            })
        }
        "cast" => {
            arity(2)?;
            cast(args[0].clone(), &arg_text(&args[1]).unwrap_or_default())
        }
        "unixepoch" | "current_timestamp" | "current_date" | "current_time" => {
            return Err(Error::Unsupported(format!(
                "{name}() (date and time functions)"
            )));
        }
        _ => return Err(Error::Unsupported(format!("the function {name}()"))),
    })
}

fn substr(s: &Value, start: &Value, len: Option<&Value>) -> Value {
    if matches!(s, Value::Null)
        || matches!(start, Value::Null)
        || len.is_some_and(|l| matches!(l, Value::Null))
    {
        return Value::Null;
    }
    let blob = matches!(s, Value::Blob(_));
    let units: Vec<Value> = if let Value::Blob(b) = s {
        b.iter().map(|x| Value::Integer(i64::from(*x))).collect()
    } else {
        arg_text(s)
            .unwrap_or_default()
            .chars()
            .map(|c| Value::Text(c.to_string()))
            .collect()
    };
    let n = units.len() as i64;
    let start = as_i64(&to_number(start).unwrap_or(Value::Integer(0)));
    let mut first = match start.cmp(&0) {
        Ordering::Greater => start - 1,
        Ordering::Less => n + start,
        Ordering::Equal => -1,
    };
    let mut count = len.map_or(n + 1, |l| {
        as_i64(&to_number(l).unwrap_or(Value::Integer(0)))
    });
    if start == 0 && len.is_some() {
        // Position 0 is before the first character: it eats one of the requested ones.
        count -= 1;
        first = 0;
    }
    if count < 0 {
        first += count;
        count = -count;
    }
    let lo = first.max(0).min(n);
    let hi = (first + count).max(0).min(n);
    let picked = &units[lo as usize..hi.max(lo) as usize];
    if blob {
        Value::Blob(picked.iter().map(|v| as_i64(v) as u8).collect())
    } else {
        Value::Text(picked.iter().filter_map(|v| v.as_text()).collect())
    }
}

/// The running state of one aggregate over a group.
#[derive(Debug, Clone)]
pub enum Accumulator {
    Count(i64),
    Sum {
        int: i64,
        real: f64,
        any: bool,
        is_real: bool,
        overflow: bool,
        total: bool,
    },
    Avg {
        sum: f64,
        n: i64,
    },
    Min(Option<Value>),
    Max(Option<Value>),
    Concat {
        out: Option<String>,
        separator: String,
    },
}

impl Accumulator {
    /// The accumulator for the aggregate `name`.
    pub fn new(name: &str) -> Result<Self> {
        Ok(match name {
            "count" => Self::Count(0),
            "sum" | "total" => Self::Sum {
                int: 0,
                real: 0.0,
                any: false,
                is_real: false,
                overflow: false,
                total: name == "total",
            },
            "avg" => Self::Avg { sum: 0.0, n: 0 },
            "min" => Self::Min(None),
            "max" => Self::Max(None),
            "group_concat" | "string_agg" => Self::Concat {
                out: None,
                separator: ",".into(),
            },
            _ => return Err(Error::Unsupported(format!("the aggregate {name}()"))),
        })
    }

    /// Feeds one row's argument (`None` for `count(*)`).
    pub fn step(&mut self, arg: Option<&Value>, separator: Option<&Value>, collation: Collation) {
        let want_less = matches!(self, Self::Min(_));
        match self {
            Self::Count(n) => {
                if arg.is_none_or(|v| !matches!(v, Value::Null)) {
                    *n += 1;
                }
            }
            Self::Sum {
                int,
                real,
                any,
                is_real,
                overflow,
                ..
            } => {
                let Some(v) = arg.and_then(to_number) else {
                    return;
                };
                *any = true;
                match v {
                    Value::Integer(i) if !*is_real => match int.checked_add(i) {
                        Some(s) => *int = s,
                        None => *overflow = true,
                    },
                    Value::Integer(i) => *real += i as f64,
                    Value::Real(r) => {
                        if !*is_real {
                            *is_real = true;
                            *real = *int as f64;
                        }
                        *real += r;
                    }
                    _ => {}
                }
            }
            Self::Avg { sum, n } => {
                if let Some(v) = arg.and_then(to_number) {
                    *sum += as_f64(&v);
                    *n += 1;
                }
            }
            Self::Min(best) | Self::Max(best) => {
                let Some(v) = arg.filter(|v| !matches!(v, Value::Null)) else {
                    return;
                };
                let better = best.as_ref().is_none_or(|b| {
                    let o = compare(v, b, collation);
                    if want_less {
                        o == Ordering::Less
                    } else {
                        o == Ordering::Greater
                    }
                });
                if better {
                    *best = Some(v.clone());
                }
            }
            Self::Concat {
                out,
                separator: sep,
            } => {
                let Some(text) = arg.and_then(to_text) else {
                    return;
                };
                if let Some(s) = separator.and_then(to_text) {
                    sep.clone_from(&s);
                }
                match out {
                    Some(o) => {
                        o.push_str(sep);
                        o.push_str(&text);
                    }
                    None => *out = Some(text),
                }
            }
        }
    }

    /// The result for the group.
    #[must_use]
    pub fn finish(&self) -> Value {
        match self {
            Self::Count(n) => Value::Integer(*n),
            Self::Sum {
                int,
                real,
                any,
                is_real,
                overflow,
                total,
            } => {
                if *overflow && !*total {
                    return Value::Null;
                }
                if !*any {
                    return if *total {
                        Value::Real(0.0)
                    } else {
                        Value::Null
                    };
                }
                if *is_real || *total {
                    Value::Real(if *is_real { *real } else { *int as f64 })
                } else {
                    Value::Integer(*int)
                }
            }
            Self::Avg { sum, n } => {
                if *n == 0 {
                    Value::Null
                } else {
                    Value::Real(sum / *n as f64)
                }
            }
            Self::Min(v) | Self::Max(v) => v.clone().unwrap_or(Value::Null),
            Self::Concat { out, .. } => out.clone().map_or(Value::Null, Value::Text),
        }
    }
}

fn hex_upper(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02X}");
        out
    })
}
