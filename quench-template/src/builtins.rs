//! The built-in filters, tests, functions and methods.

use crate::Environment;
use crate::error::{Error, ErrorKind, Result};
use crate::value::{self, Num, compare, display, eq, from_int, num, truthy, type_name};
use quench_serde::{Map, Value, json};
use std::cmp::Ordering;

/// The arguments of a filter or function call: positional, then keyword.
#[derive(Debug, Default, Clone)]
pub struct Args {
    pub positional: Vec<Value>,
    pub keyword: Vec<(String, Value)>,
}

impl Args {
    /// Argument `index`, or the keyword `name`.
    #[must_use]
    pub fn get(&self, index: usize, name: &str) -> Option<&Value> {
        self.positional
            .get(index)
            .or_else(|| self.keyword.iter().find(|(k, _)| k == name).map(|(_, v)| v))
    }

    fn string(&self, index: usize, name: &str, default: &str) -> Result<String> {
        match self.get(index, name) {
            None | Some(Value::Null) => Ok(default.to_string()),
            Some(Value::String(s)) => Ok(s.clone()),
            Some(other) => Ok(display(other)),
        }
    }

    fn flag(&self, index: usize, name: &str, default: bool) -> bool {
        self.get(index, name).map_or(default, truthy)
    }

    fn int(&self, index: usize, name: &str, default: i64) -> Result<i64> {
        match self.get(index, name) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => as_i64(v),
        }
    }
}

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, message)
}

fn as_i64(value: &Value) -> Result<i64> {
    match num(value) {
        Some(Num::Int(n)) => i64::try_from(n).map_err(|_| invalid("integer out of range")),
        Some(Num::Float(f)) => Ok(f as i64),
        None => Err(invalid(format!(
            "expected a number, found {}",
            type_name(value)
        ))),
    }
}

fn as_str(value: &Value) -> Result<&str> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(invalid(format!(
            "expected a string, found {}",
            type_name(other)
        ))),
    }
}

/// The items of something iterable: elements of a list, keys of a map, characters of a string.
pub fn items(value: &Value) -> Result<Vec<Value>> {
    match value {
        Value::Array(a) => Ok(a.clone()),
        Value::Object(m) => Ok(m.keys().map(|k| Value::String(k.clone())).collect()),
        Value::String(s) => Ok(s.chars().map(|c| Value::String(c.to_string())).collect()),
        Value::Null => Ok(Vec::new()),
        other => Err(invalid(format!("{} is not iterable", type_name(other)))),
    }
}

fn attribute(value: &Value, path: &str) -> Value {
    let mut current = value;
    for part in path.split('.') {
        let next = match current {
            Value::Object(m) => m.get(part),
            Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
            _ => None,
        };
        match next {
            Some(v) => current = v,
            None => return Value::Null,
        }
    }
    current.clone()
}

fn sort_key(value: &Value, case_sensitive: bool) -> Value {
    match value {
        Value::String(s) if !case_sensitive => Value::String(s.to_lowercase()),
        other => other.clone(),
    }
}

fn order(a: &Value, b: &Value) -> Ordering {
    compare(a, b).unwrap_or(Ordering::Equal)
}

fn title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

fn indent(text: &str, args: &Args) -> Result<Value> {
    let width = match args.get(0, "width") {
        Some(Value::String(s)) => s.clone(),
        Some(v) => " ".repeat(usize::try_from(as_i64(v)?.max(0)).unwrap_or(0)),
        None => "    ".to_string(),
    };
    let first = args.flag(1, "first", false);
    let blank = args.flag(2, "blank", false);
    let mut out = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let skip = (i == 0 && !first) || (line.is_empty() && !blank);
        if !skip {
            out.push_str(&width);
        }
        out.push_str(line);
    }
    Ok(Value::String(out))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn round(value: &Value, args: &Args) -> Result<Value> {
    let Some(n) = num(value) else {
        return Err(invalid("round needs a number"));
    };
    let precision = i32::try_from(args.int(0, "precision", 0)?).unwrap_or(0);
    let method = args.string(1, "method", "common")?;
    let x = match n {
        Num::Int(i) => i as f64,
        Num::Float(f) => f,
    };
    let scale = 10f64.powi(precision);
    let scaled = x * scale;
    let rounded = match method.as_str() {
        "ceil" => scaled.ceil(),
        "floor" => scaled.floor(),
        _ => scaled.round(),
    };
    Ok(Value::Float(rounded / scale))
}

fn truncate(text: &str, args: &Args) -> Result<Value> {
    let length = usize::try_from(args.int(0, "length", 255)?.max(0)).unwrap_or(0);
    let kill = args.flag(1, "killwords", false);
    let end = args.string(2, "end", "...")?;
    if text.chars().count() <= length {
        return Ok(Value::String(text.to_string()));
    }
    let keep = length.saturating_sub(end.chars().count());
    let cut: String = text.chars().take(keep).collect();
    let cut = if kill {
        cut
    } else {
        cut.rsplit_once(' ')
            .map_or(cut.clone(), |(head, _)| head.to_string())
    };
    Ok(Value::String(format!("{cut}{end}")))
}

/// Runs the built-in filter `name`; `None` when there is no such filter.
#[allow(clippy::too_many_lines)]
pub fn filter(env: &Environment, name: &str, v: &Value, args: &Args) -> Option<Result<Value>> {
    let text = || as_str(v).map(str::to_string);
    Some(match name {
        "upper" => text().map(|s| Value::String(s.to_uppercase())),
        "lower" => text().map(|s| Value::String(s.to_lowercase())),
        "title" => text().map(|s| Value::String(title(&s))),
        "capitalize" => text().map(|s| {
            let mut chars = s.chars();
            Value::String(chars.next().map_or_else(String::new, |c| {
                c.to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            }))
        }),
        "trim" | "strip" => text().map(|s| {
            Value::String(match args.get(0, "chars") {
                Some(Value::String(chars)) => s.trim_matches(|c| chars.contains(c)).to_string(),
                _ => s.trim().to_string(),
            })
        }),
        "escape" | "e" | "safe" | "forceescape" => Ok(v.clone()),
        "string" => Ok(Value::String(display(v))),
        "indent" => text().and_then(|s| indent(&s, args)),
        "replace" => text().and_then(|s| {
            let from = args.string(0, "old", "")?;
            let to = args.string(1, "new", "")?;
            Ok(Value::String(match args.get(2, "count") {
                Some(c) => s.replacen(&from, &to, usize::try_from(as_i64(c)?.max(0)).unwrap_or(0)),
                None => s.replace(&from, &to),
            }))
        }),
        "truncate" => text().and_then(|s| truncate(&s, args)),
        "urlencode" => text().map(|s| Value::String(percent_encode(&s))),
        "split" => text().and_then(|s| {
            let parts: Vec<Value> = match args.get(0, "sep") {
                Some(Value::String(sep)) => match args.get(1, "maxsplit") {
                    Some(n) => s
                        .splitn(
                            usize::try_from(as_i64(n)?.max(0)).unwrap_or(0) + 1,
                            sep.as_str(),
                        )
                        .map(|p| Value::String(p.to_string()))
                        .collect(),
                    None => s
                        .split(sep.as_str())
                        .map(|p| Value::String(p.to_string()))
                        .collect(),
                },
                _ => s
                    .split_whitespace()
                    .map(|p| Value::String(p.to_string()))
                    .collect(),
            };
            Ok(Value::Array(parts))
        }),
        "length" | "count" => match v {
            Value::String(s) => Ok(Value::from(s.chars().count() as u64)),
            Value::Array(a) => Ok(Value::from(a.len() as u64)),
            Value::Object(m) => Ok(Value::from(m.iter().len() as u64)),
            other => Err(invalid(format!("{} has no length", type_name(other)))),
        },
        "list" => items(v).map(Value::Array),
        "first" => items(v).map(|i| i.into_iter().next().unwrap_or(Value::Null)),
        "last" => items(v).map(|i| i.into_iter().next_back().unwrap_or(Value::Null)),
        "reverse" => match v {
            Value::String(s) => Ok(Value::String(s.chars().rev().collect())),
            other => items(other).map(|mut i| {
                i.reverse();
                Value::Array(i)
            }),
        },
        "keys" => match v {
            Value::Object(m) => Ok(Value::Array(
                m.keys().map(|k| Value::String(k.clone())).collect(),
            )),
            other => Err(invalid(format!("{} has no keys", type_name(other)))),
        },
        "values" => match v {
            Value::Object(m) => Ok(Value::Array(m.values().cloned().collect())),
            other => Err(invalid(format!("{} has no values", type_name(other)))),
        },
        "items" => match v {
            Value::Object(m) => Ok(Value::Array(
                m.iter()
                    .map(|(k, v)| Value::Array(vec![Value::String(k.clone()), v.clone()]))
                    .collect(),
            )),
            other => Err(invalid(format!("{} has no items", type_name(other)))),
        },
        "dictsort" => match v {
            Value::Object(m) => {
                let case_sensitive = args.flag(0, "case_sensitive", false);
                let by_value = args.string(1, "by", "key").is_ok_and(|b| b == "value");
                let mut pairs: Vec<(Value, Value)> = m
                    .iter()
                    .map(|(k, v)| (Value::String(k.clone()), v.clone()))
                    .collect();
                pairs.sort_by(|a, b| {
                    if by_value {
                        order(
                            &sort_key(&a.1, case_sensitive),
                            &sort_key(&b.1, case_sensitive),
                        )
                    } else {
                        order(
                            &sort_key(&a.0, case_sensitive),
                            &sort_key(&b.0, case_sensitive),
                        )
                    }
                });
                if args.flag(2, "reverse", false) {
                    pairs.reverse();
                }
                Ok(Value::Array(
                    pairs
                        .into_iter()
                        .map(|(k, v)| Value::Array(vec![k, v]))
                        .collect(),
                ))
            }
            other => Err(invalid(format!(
                "cannot sort a {} as a map",
                type_name(other)
            ))),
        },
        "sort" => items(v).map(|mut list| {
            let case_sensitive = args.flag(1, "case_sensitive", false);
            let attr = args.get(2, "attribute").map(display);
            let key = |x: &Value| {
                let base = attr.as_ref().map_or_else(|| x.clone(), |a| attribute(x, a));
                sort_key(&base, case_sensitive)
            };
            list.sort_by(|a, b| order(&key(a), &key(b)));
            if args.flag(0, "reverse", false) {
                list.reverse();
            }
            Value::Array(list)
        }),
        "unique" => items(v).map(|list| {
            let mut seen: Vec<Value> = Vec::new();
            for item in list {
                if !seen.iter().any(|s| eq(s, &item)) {
                    seen.push(item);
                }
            }
            Value::Array(seen)
        }),
        "join" => items(v).and_then(|list| {
            let sep = args.string(0, "d", "")?;
            let attr = args.get(1, "attribute").map(display);
            let parts: Vec<String> = list
                .iter()
                .map(|x| display(&attr.as_ref().map_or_else(|| x.clone(), |a| attribute(x, a))))
                .collect();
            Ok(Value::String(parts.join(&sep)))
        }),
        "sum" => items(v).and_then(|list| {
            let attr = args.get(0, "attribute").map(display);
            let mut total = args.get(1, "start").cloned().unwrap_or(Value::from(0));
            for item in list {
                let x = attr.as_ref().map_or(item.clone(), |a| attribute(&item, a));
                total = value::add(&total, &x)?;
            }
            Ok(total)
        }),
        "min" | "max" => items(v).map(|list| {
            let pick = if name == "min" {
                Ordering::Less
            } else {
                Ordering::Greater
            };
            list.into_iter()
                .reduce(|a, b| if order(&b, &a) == pick { b } else { a })
                .unwrap_or(Value::Null)
        }),
        "batch" => items(v).and_then(|list| {
            let size = usize::try_from(args.int(0, "linecount", 1)?.max(1)).unwrap_or(1);
            let fill = args.get(1, "fill_with").cloned();
            let mut rows: Vec<Value> = Vec::new();
            for chunk in list.chunks(size) {
                let mut row = chunk.to_vec();
                if let Some(f) = &fill {
                    row.resize(size, f.clone());
                }
                rows.push(Value::Array(row));
            }
            Ok(Value::Array(rows))
        }),
        "abs" => match num(v) {
            Some(Num::Int(i)) => from_int(i.abs()),
            Some(Num::Float(f)) => Ok(Value::Float(f.abs())),
            None => Err(invalid("abs needs a number")),
        },
        "round" => round(v, args),
        "int" => Ok(match v {
            Value::String(s) => s
                .trim()
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| s.trim().parse::<f64>().map(|f| Value::from(f as i64)))
                .unwrap_or_else(|_| args.get(0, "default").cloned().unwrap_or(Value::from(0))),
            other => match num(other) {
                Some(Num::Int(i)) => from_int(i).unwrap_or(Value::from(0)),
                Some(Num::Float(f)) => Value::from(f as i64),
                None => args.get(0, "default").cloned().unwrap_or(Value::from(0)),
            },
        }),
        "float" => Ok(match v {
            Value::String(s) => s.trim().parse::<f64>().map_or_else(
                |_| args.get(0, "default").cloned().unwrap_or(Value::Float(0.0)),
                Value::Float,
            ),
            other => match num(other) {
                Some(Num::Int(i)) => Value::Float(i as f64),
                Some(Num::Float(f)) => Value::Float(f),
                None => args.get(0, "default").cloned().unwrap_or(Value::Float(0.0)),
            },
        }),
        "tojson" => Ok(Value::String(json::to_string(v))),
        "attr" => Ok(attribute(
            v,
            &args.string(0, "name", "").unwrap_or_default(),
        )),
        "select" | "reject" => select(env, v, args, name == "reject", false),
        "selectattr" | "rejectattr" => select(env, v, args, name == "rejectattr", true),
        "map" => map(env, v, args),
        _ => return None,
    })
}

/// `select`, `reject`, `selectattr`, `rejectattr`.
fn select(env: &Environment, v: &Value, args: &Args, reject: bool, by_attr: bool) -> Result<Value> {
    let list = items(v)?;
    let (attr, rest) = if by_attr {
        let name = args
            .positional
            .first()
            .map(display)
            .ok_or_else(|| invalid("an attribute name is needed"))?;
        (Some(name), 1)
    } else {
        (None, 0)
    };
    let test = args.positional.get(rest).map(display);
    let extra: Vec<Value> = args.positional.iter().skip(rest + 1).cloned().collect();
    let mut out = Vec::new();
    for item in list {
        let subject = attr
            .as_ref()
            .map_or_else(|| item.clone(), |a| attribute(&item, a));
        let passed = match &test {
            Some(t) => env.apply_test(t, Some(&subject), &extra)?,
            None => truthy(&subject),
        };
        if passed != reject {
            out.push(item);
        }
    }
    Ok(Value::Array(out))
}

fn map(env: &Environment, v: &Value, args: &Args) -> Result<Value> {
    let list = items(v)?;
    let mut out = Vec::new();
    if let Some((_, attr)) = args.keyword.iter().find(|(k, _)| k == "attribute") {
        let attr = display(attr);
        let default = args
            .keyword
            .iter()
            .find(|(k, _)| k == "default")
            .map(|(_, v)| v.clone());
        for item in &list {
            let found = attribute(item, &attr);
            out.push(if matches!(found, Value::Null) {
                default.clone().unwrap_or(found)
            } else {
                found
            });
        }
    } else if let Some(name) = args.positional.first().map(display) {
        let rest = Args {
            positional: args.positional.iter().skip(1).cloned().collect(),
            keyword: Vec::new(),
        };
        for item in &list {
            out.push(env.apply_filter(&name, item, &rest)?);
        }
    } else {
        return Err(invalid("map needs a filter name or `attribute=`"));
    }
    Ok(Value::Array(out))
}

/// Runs the built-in test `name` on a defined value; `None` when there is no such test.
pub fn test(env: &Environment, name: &str, v: &Value, args: &[Value]) -> Option<Result<bool>> {
    let arg = || {
        args.first()
            .ok_or_else(|| invalid(format!("test `{name}` needs an argument")))
    };
    Some(match name {
        "none" => Ok(matches!(v, Value::Null)),
        "string" => Ok(matches!(v, Value::String(_))),
        "number" => Ok(matches!(
            v,
            Value::Int(_) | Value::UInt(_) | Value::Float(_)
        )),
        "integer" => Ok(matches!(v, Value::Int(_) | Value::UInt(_))),
        "float" => Ok(matches!(v, Value::Float(_))),
        "boolean" => Ok(matches!(v, Value::Bool(_))),
        "mapping" => Ok(matches!(v, Value::Object(_))),
        "sequence" => Ok(matches!(v, Value::Array(_) | Value::String(_))),
        "iterable" => Ok(matches!(
            v,
            Value::Array(_) | Value::String(_) | Value::Object(_)
        )),
        "true" => Ok(matches!(v, Value::Bool(true))),
        "false" => Ok(matches!(v, Value::Bool(false))),
        "callable" => Ok(false),
        "odd" => Ok(matches!(num(v), Some(Num::Int(i)) if i % 2 != 0)),
        "even" => Ok(matches!(num(v), Some(Num::Int(i)) if i % 2 == 0)),
        "divisibleby" => arg().and_then(|d| match (num(v), num(d)) {
            (Some(Num::Int(a)), Some(Num::Int(b))) if b != 0 => Ok(a % b == 0),
            _ => Err(invalid("divisibleby needs non-zero integers")),
        }),
        "eq" | "equalto" | "sameas" => arg().map(|a| eq(v, a)),
        "ne" => arg().map(|a| !eq(v, a)),
        "lt" | "lessthan" => arg().map(|a| compare(v, a) == Some(Ordering::Less)),
        "le" => arg().map(|a| matches!(compare(v, a), Some(Ordering::Less | Ordering::Equal))),
        "gt" | "greaterthan" => arg().map(|a| compare(v, a) == Some(Ordering::Greater)),
        "ge" => arg().map(|a| matches!(compare(v, a), Some(Ordering::Greater | Ordering::Equal))),
        "in" => arg().and_then(|a| value::contains(a, v)),
        "lower" => Ok(matches!(v, Value::String(s) if s.chars().all(|c| !c.is_uppercase()))),
        "upper" => Ok(matches!(v, Value::String(s) if s.chars().all(|c| !c.is_lowercase()))),
        "startingwith" => arg().map(
            |a| matches!((v, a), (Value::String(s), Value::String(p)) if s.starts_with(p.as_str())),
        ),
        "endingwith" => arg().map(
            |a| matches!((v, a), (Value::String(s), Value::String(p)) if s.ends_with(p.as_str())),
        ),
        _ => {
            let _ = env;
            return None;
        }
    })
}

/// `range`, `dict`: functions available without registration.
pub fn function(name: &str, args: &Args) -> Option<Result<Value>> {
    match name {
        "range" => Some((|| {
            let numbers: Result<Vec<i64>> = args.positional.iter().map(as_i64).collect();
            let numbers = numbers?;
            let (start, stop, step) = match numbers.as_slice() {
                [stop] => (0, *stop, 1),
                [start, stop] => (*start, *stop, 1),
                [start, stop, step] => (*start, *stop, *step),
                _ => return Err(invalid("range takes one to three numbers")),
            };
            if step == 0 {
                return Err(invalid("range step cannot be zero"));
            }
            let mut out = Vec::new();
            let mut n = start;
            while (step > 0 && n < stop) || (step < 0 && n > stop) {
                out.push(Value::from(n));
                n += step;
                if out.len() > 100_000 {
                    return Err(invalid("range is too long"));
                }
            }
            Ok(Value::Array(out))
        })()),
        "dict" => Some(Ok(Value::Object(
            args.keyword
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<Map>(),
        ))),
        _ => None,
    }
}

/// Methods called on values: `"a b".split()`, `d.items()`, ...
pub fn method(v: &Value, name: &str, args: &Args) -> Option<Result<Value>> {
    let text = |s: &str| Value::String(s.to_string());
    match (v, name) {
        (Value::String(s), "upper") => Some(Ok(text(&s.to_uppercase()))),
        (Value::String(s), "lower") => Some(Ok(text(&s.to_lowercase()))),
        (Value::String(s), "strip") => Some(Ok(text(s.trim()))),
        (Value::String(s), "lstrip") => Some(Ok(text(s.trim_start()))),
        (Value::String(s), "rstrip") => Some(Ok(text(s.trim_end()))),
        (Value::String(s), "startswith") => Some(
            args.string(0, "prefix", "")
                .map(|p| Value::Bool(s.starts_with(&p))),
        ),
        (Value::String(s), "endswith") => Some(
            args.string(0, "suffix", "")
                .map(|p| Value::Bool(s.ends_with(&p))),
        ),
        (Value::String(s), "split") => {
            let parts = match args.get(0, "sep") {
                Some(Value::String(sep)) => s.split(sep.as_str()).map(text).collect(),
                _ => s.split_whitespace().map(text).collect(),
            };
            Some(Ok(Value::Array(parts)))
        }
        (Value::String(s), "replace") => Some((|| {
            Ok(text(&s.replace(
                &args.string(0, "old", "")?,
                &args.string(1, "new", "")?,
            )))
        })()),
        (Value::String(s), "join") => Some(
            args.positional
                .first()
                .ok_or_else(|| invalid("join needs a sequence"))
                .and_then(items)
                .map(|list| text(&list.iter().map(display).collect::<Vec<_>>().join(s))),
        ),
        (Value::String(s), "capitalize") => {
            let mut chars = s.chars();
            Some(Ok(text(&chars.next().map_or_else(String::new, |c| {
                c.to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect()
            }))))
        }
        (Value::String(s), "title") => Some(Ok(text(&title(s)))),
        (Value::Object(m), "items") => Some(Ok(Value::Array(
            m.iter()
                .map(|(k, v)| Value::Array(vec![text(k), v.clone()]))
                .collect(),
        ))),
        (Value::Object(m), "keys") => Some(Ok(Value::Array(m.keys().map(|k| text(k)).collect()))),
        (Value::Object(m), "values") => Some(Ok(Value::Array(m.values().cloned().collect()))),
        (Value::Object(m), "get") => Some(Ok(args
            .positional
            .first()
            .and_then(|k| match k {
                Value::String(k) => m.get(k).cloned(),
                _ => None,
            })
            .or_else(|| args.positional.get(1).cloned())
            .unwrap_or(Value::Null))),
        _ => None,
    }
}
