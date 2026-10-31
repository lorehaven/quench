//! Evaluating expressions: name resolution against the rows in scope, then the operators.

use std::cmp::Ordering;

use super::exec::Exec;
use super::ops::{self, CompareRules};
use super::value::{self, Affinity, Collation};
use crate::error::{Error, Result};
use crate::record::Value;
use crate::sql::ast::{BinaryOp, Expr, LikeKind, Param, UnaryOp};

/// One column in scope.
#[derive(Debug, Clone)]
pub struct ScopeCol {
    /// The table's name or alias, lower case.
    pub table: String,
    /// Lower case, for matching.
    pub name: String,
    /// As declared, for result column names.
    pub label: String,
    pub affinity: Affinity,
    pub collation: Collation,
    /// The rowid (`*` does not show it).
    pub hidden: bool,
}

/// The columns a row has, in order.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub cols: Vec<ScopeCol>,
}

impl Scope {
    /// The position of a column: `Ok(None)` when no such name, an error when it is ambiguous.
    pub fn resolve(&self, table: Option<&str>, name: &str) -> Result<Option<usize>> {
        let name = name.to_ascii_lowercase();
        let table = table.map(str::to_ascii_lowercase);
        let find = |rowid: bool| -> Result<Option<usize>> {
            let mut hits = self.cols.iter().enumerate().filter(|(_, c)| {
                c.hidden == rowid && c.name == name && table.as_ref().is_none_or(|t| *t == c.table)
            });
            let first = hits.next().map(|(i, _)| i);
            if hits.next().is_some() {
                return Err(Error::Syntax {
                    pos: 0,
                    message: format!("ambiguous column name: {name}"),
                });
            }
            Ok(first)
        };
        if let Some(i) = find(false)? {
            return Ok(Some(i));
        }
        if matches!(name.as_str(), "rowid" | "_rowid_" | "oid") {
            let wanted = self
                .cols
                .iter()
                .position(|c| c.hidden && table.as_ref().is_none_or(|t| *t == c.table));
            return Ok(wanted);
        }
        Ok(None)
    }
}

/// A row and the names of its values.
pub type Frame<'a> = (&'a Scope, &'a [Value]);

/// The values of the aggregate calls of the current group.
#[derive(Debug, Clone, Copy)]
pub struct Aggregates<'a> {
    pub calls: &'a [Expr],
    pub values: &'a [Value],
}

/// Bound parameters.
#[derive(Debug, Clone, Copy)]
pub struct Params<'a> {
    pub positional: &'a [Value],
    pub named: &'a [(&'a str, Value)],
}

impl Params<'_> {
    pub(crate) fn get(&self, p: &Param) -> Result<Value> {
        match p {
            Param::Index(i) => self.positional.get(i - 1).cloned().ok_or_else(|| {
                Error::Unsupported(format!(
                    "parameter ?{i} has no value ({} given)",
                    self.positional.len()
                ))
            }),
            Param::Named(n) => self
                .named
                .iter()
                .find(|(k, _)| {
                    k.trim_start_matches([':', '@', '$'])
                        .eq_ignore_ascii_case(n)
                })
                .map(|(_, v)| v.clone())
                .ok_or_else(|| Error::Unsupported(format!("parameter :{n} has no value"))),
        }
    }
}

fn resolve_in(frames: &[Frame], table: Option<&str>, name: &str) -> Result<Option<(usize, usize)>> {
    for (f, (scope, _)) in frames.iter().enumerate() {
        if let Some(i) = scope.resolve(table, name)? {
            return Ok(Some((f, i)));
        }
    }
    Ok(None)
}

/// The affinity an expression has in a comparison: a column's own, `CAST`'s type, else none.
fn affinity_of_expr(e: &Expr, frames: &[Frame]) -> Affinity {
    match e {
        Expr::Column { table, name } => resolve_in(frames, table.as_deref(), name)
            .ok()
            .flatten()
            .map_or(Affinity::Blob, |(f, i)| frames[f].0.cols[i].affinity),
        Expr::Cast { type_name, .. } => value::affinity_of(Some(type_name)),
        Expr::Collate { expr, .. } | Expr::Unary(UnaryOp::Plus, expr) => {
            affinity_of_expr(expr, frames)
        }
        _ => Affinity::Blob,
    }
}

/// The collation of an expression: `COLLATE`, else a column's, else none.
pub fn collation_of(e: &Expr, frames: &[Frame]) -> Option<Collation> {
    match e {
        Expr::Collate { collation, .. } => Collation::by_name(collation),
        Expr::Column { table, name } => {
            let (f, i) = resolve_in(frames, table.as_deref(), name).ok().flatten()?;
            let c = frames[f].0.cols[i].collation;
            (c != Collation::Binary).then_some(c)
        }
        Expr::Unary(UnaryOp::Plus, inner) => collation_of(inner, frames),
        _ => None,
    }
}

fn rules(l: &Expr, r: &Expr, frames: &[Frame]) -> CompareRules {
    CompareRules {
        left: affinity_of_expr(l, frames),
        right: affinity_of_expr(r, frames),
        collation: collation_of(l, frames)
            .or_else(|| collation_of(r, frames))
            .unwrap_or_default(),
    }
}

fn bool_value(b: bool) -> Value {
    Value::Integer(i64::from(b))
}

impl Exec<'_> {
    /// Evaluates `e` with the rows in `frames` (innermost first) and, in a grouped query, the aggregates.
    pub fn eval(&mut self, e: &Expr, frames: &[Frame], aggs: Option<Aggregates>) -> Result<Value> {
        Ok(match e {
            Expr::Literal(v) => v.clone(),
            Expr::Param(p) => self.params.get(p)?,
            Expr::Column { table, name } => {
                let (f, i) =
                    resolve_in(frames, table.as_deref(), name)?.ok_or_else(|| Error::Syntax {
                        pos: 0,
                        message: format!(
                            "no such column: {}{name}",
                            table.as_ref().map_or(String::new(), |t| format!("{t}."))
                        ),
                    })?;
                frames[f].1[i].clone()
            }
            Expr::Unary(op, inner) => {
                let v = self.eval(inner, frames, aggs)?;
                match op {
                    UnaryOp::Not => value::truth(&v).map_or(Value::Null, |b| bool_value(!b)),
                    UnaryOp::Plus => v,
                    UnaryOp::Neg => match value::to_number(&v) {
                        None => Value::Null,
                        Some(Value::Integer(i)) => i
                            .checked_neg()
                            .map_or(Value::Real(-(i as f64)), Value::Integer),
                        Some(Value::Real(r)) => Value::Real(-r),
                        Some(other) => other,
                    },
                    UnaryOp::BitNot => value::to_number(&v).map_or(Value::Null, |n| match n {
                        Value::Integer(i) => Value::Integer(!i),
                        Value::Real(r) => Value::Integer(!value::real_to_i64(r)),
                        other => other,
                    }),
                }
            }
            Expr::Binary(op, l, r) => self.binary(*op, l, r, frames, aggs)?,
            Expr::IsNull { expr, negated } => {
                let null = matches!(self.eval(expr, frames, aggs)?, Value::Null);
                bool_value(null != *negated)
            }
            Expr::Between {
                expr,
                low,
                high,
                negated,
            } => {
                let v = self.eval(expr, frames, aggs)?;
                let lo = self.eval(low, frames, aggs)?;
                let hi = self.eval(high, frames, aggs)?;
                let ge = ops::comparison(BinaryOp::Ge, &v, &lo, rules(expr, low, frames));
                let le = ops::comparison(BinaryOp::Le, &v, &hi, rules(expr, high, frames));
                let both = ops::logic(BinaryOp::And, value::truth(&ge), value::truth(&le));
                if *negated {
                    value::truth(&both).map_or(Value::Null, |b| bool_value(!b))
                } else {
                    both
                }
            }
            Expr::InList {
                expr,
                list,
                negated,
            } => {
                let v = self.eval(expr, frames, aggs)?;
                let mut candidates = Vec::with_capacity(list.len());
                for item in list {
                    candidates.push((self.eval(item, frames, aggs)?, rules(expr, item, frames)));
                }
                membership(&v, &candidates, *negated)
            }
            Expr::InSelect {
                expr,
                select,
                negated,
            } => {
                let v = self.eval(expr, frames, aggs)?;
                let rows = self.select(select, frames)?;
                let r = CompareRules {
                    left: affinity_of_expr(expr, frames),
                    right: Affinity::Blob,
                    collation: collation_of(expr, frames).unwrap_or_default(),
                };
                let candidates: Vec<(Value, CompareRules)> = rows
                    .rows
                    .into_iter()
                    .filter_map(|mut row| (!row.is_empty()).then(|| (row.swap_remove(0), r)))
                    .collect();
                membership(&v, &candidates, *negated)
            }
            Expr::Like {
                kind,
                expr,
                pattern,
                escape,
                negated,
            } => {
                let v = self.eval(expr, frames, aggs)?;
                let p = self.eval(pattern, frames, aggs)?;
                let esc = match escape {
                    Some(e) => Some(self.eval(e, frames, aggs)?),
                    None => None,
                };
                let (Some(text), Some(pat)) = (value::to_text(&v), value::to_text(&p)) else {
                    return Ok(Value::Null);
                };
                let esc_char = match &esc {
                    Some(Value::Null) => return Ok(Value::Null),
                    Some(x) => value::to_text(x).and_then(|s| s.chars().next()),
                    None => None,
                };
                let m = ops::pattern_match(*kind == LikeKind::Glob, &text, &pat, esc_char);
                bool_value(m != *negated)
            }
            Expr::Case {
                operand,
                whens,
                otherwise,
            } => {
                let base = match operand {
                    Some(o) => Some(self.eval(o, frames, aggs)?),
                    None => None,
                };
                for (when, then) in whens {
                    let w = self.eval(when, frames, aggs)?;
                    let hit = match (&base, operand) {
                        (Some(b), Some(o)) => {
                            value::truth(&ops::comparison(
                                BinaryOp::Eq,
                                b,
                                &w,
                                rules(o, when, frames),
                            )) == Some(true)
                        }
                        _ => value::truth(&w) == Some(true),
                    };
                    if hit {
                        return self.eval(then, frames, aggs);
                    }
                }
                match otherwise {
                    Some(o) => self.eval(o, frames, aggs)?,
                    None => Value::Null,
                }
            }
            Expr::Function {
                name,
                args,
                distinct,
                star,
            } => {
                if ops::is_aggregate(name, args.len()) {
                    let a = aggs.ok_or_else(|| {
                        Error::Unsupported(format!("misuse of aggregate function {name}()"))
                    })?;
                    let at = a.calls.iter().position(|c| c == e).ok_or_else(|| {
                        Error::Unsupported(format!("misuse of aggregate function {name}()"))
                    })?;
                    return Ok(a.values[at].clone());
                }
                if *star || *distinct {
                    return Err(Error::Unsupported(format!(
                        "{name}(*) or DISTINCT on a scalar function"
                    )));
                }
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, frames, aggs)?);
                }
                ops::call_scalar(name, &values)?
            }
            Expr::Cast { expr, type_name } => {
                value::cast(self.eval(expr, frames, aggs)?, type_name)
            }
            Expr::Collate { expr, .. } => self.eval(expr, frames, aggs)?,
            Expr::Subquery(select) => {
                let mut rows = self.select(select, frames)?;
                if rows.rows.is_empty() || rows.rows[0].is_empty() {
                    Value::Null
                } else {
                    rows.rows.swap_remove(0).swap_remove(0)
                }
            }
            Expr::Exists(select) => bool_value(!self.select(select, frames)?.rows.is_empty()),
        })
    }

    fn binary(
        &mut self,
        op: BinaryOp,
        l: &Expr,
        r: &Expr,
        frames: &[Frame],
        aggs: Option<Aggregates>,
    ) -> Result<Value> {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            let a = value::truth(&self.eval(l, frames, aggs)?);
            // Short circuit when the left side decides.
            if (op == BinaryOp::And && a == Some(false)) || (op == BinaryOp::Or && a == Some(true))
            {
                return Ok(bool_value(a == Some(true)));
            }
            let b = value::truth(&self.eval(r, frames, aggs)?);
            return Ok(ops::logic(op, a, b));
        }
        let a = self.eval(l, frames, aggs)?;
        let b = self.eval(r, frames, aggs)?;
        Ok(match op {
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Is
            | BinaryOp::IsNot => ops::comparison(op, &a, &b, rules(l, r, frames)),
            _ => ops::arithmetic(op, &a, &b),
        })
    }
}

/// `v IN (candidates)` under three-valued logic.
fn membership(v: &Value, candidates: &[(Value, CompareRules)], negated: bool) -> Value {
    if candidates.is_empty() {
        return bool_value(negated);
    }
    if matches!(v, Value::Null) {
        return Value::Null;
    }
    let mut saw_null = false;
    for (c, rules) in candidates {
        if matches!(c, Value::Null) {
            saw_null = true;
            continue;
        }
        if value::truth(&ops::comparison(BinaryOp::Eq, v, c, *rules)) == Some(true) {
            return bool_value(!negated);
        }
    }
    if saw_null {
        Value::Null
    } else {
        bool_value(negated)
    }
}

/// Orders two output values for `ORDER BY`: NULL is the smallest.
pub fn order_compare(a: &Value, b: &Value, collation: Collation) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) => Ordering::Less,
        (_, Value::Null) => Ordering::Greater,
        _ => value::compare(a, b, collation),
    }
}
