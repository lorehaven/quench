//! Running a `SELECT`: sources and joins with the access path each one gets, grouping and aggregates,
//! `DISTINCT`, `ORDER BY` and `LIMIT`; and the scans under it.

use std::collections::BTreeMap;

use super::catalog::{Catalog, Table};
use super::eval::{Aggregates, Frame, Params, Scope, ScopeCol, collation_of, order_compare};
use super::ops::{self, Accumulator};
use super::value::{self, Affinity, Collation};
use crate::Database;
use crate::error::{Error, Result};
use crate::record::{self, Value};
use crate::sql::ast::{
    BinaryOp, CompoundOp, Expr, From, JoinConstraint, JoinKind, NullsOrder, OrderTerm,
    ResultColumn, Select, SelectCore, TableSource,
};
use crate::tree;

/// The result of a query: column names and the rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// Runs statements against a database with a catalog.
#[derive(Debug)]
pub struct Exec<'a> {
    pub db: &'a mut Database,
    pub catalog: &'a Catalog,
    pub params: Params<'a>,
    pub last_rowid: i64,
    pub changes: u64,
}

/// How many rows a scan fetches at a time.
const BATCH: usize = 256;

// ---- access paths

#[derive(Debug, Clone)]
pub enum Bound {
    None,
    At(Expr, bool),
}

#[derive(Debug, Clone)]
pub enum Access {
    Full,
    RowidEq(Expr),
    RowidRange {
        lower: Bound,
        upper: Bound,
    },
    Index {
        index: usize,
        eq: Vec<Expr>,
        lower: Bound,
        upper: Bound,
    },
}

#[derive(Debug, Clone)]
enum Prepared {
    Empty,
    Point(i64),
    Rowids {
        to: i64,
    },
    Index {
        eq: Vec<Value>,
        lower: Option<(Value, bool)>,
        upper: Option<(Value, bool)>,
    },
}

#[derive(Debug, Default)]
pub struct Cursor {
    /// Rows to fetch next; starts small when only a few are wanted and doubles up to [`BATCH`].
    batch: usize,
    prepared: Option<Prepared>,
    next_rowid: Option<i64>,
    after: Option<Vec<Value>>,
    done: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ColKey {
    Rowid,
    Col(usize),
}

/// A source in a FROM clause.
struct Source<'t> {
    kind: SourceKind<'t>,
    offset: usize,
    left: bool,
    on: Option<Expr>,
    /// Columns that `USING` merged into an earlier source (`*` skips them).
    merged: Vec<usize>,
    alias: String,
}

enum SourceKind<'t> {
    Table(&'t Table),
    Derived(Vec<Vec<Value>>),
}

/// The state while a FROM clause is joined.
struct Join<'t, 'e> {
    sources: Vec<Source<'t>>,
    accesses: Vec<Access>,
    scope: Scope,
    where_clause: Option<&'e Expr>,
    row: Vec<Value>,
    out: Vec<Vec<Value>>,
    need: Option<usize>,
}

fn conjuncts<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    if let Expr::Binary(BinaryOp::And, a, b) = e {
        conjuncts(a, out);
        conjuncts(b, out);
    } else {
        out.push(e);
    }
}

fn table_scope(table: &Table, alias: &str) -> Scope {
    let a = alias.to_ascii_lowercase();
    let mut cols: Vec<ScopeCol> = table
        .columns
        .iter()
        .map(|c| ScopeCol {
            table: a.clone(),
            name: c.name.to_ascii_lowercase(),
            label: c.name.clone(),
            affinity: c.affinity,
            collation: c.collation,
            hidden: false,
        })
        .collect();
    cols.push(ScopeCol {
        table: a,
        name: "rowid".into(),
        label: "rowid".into(),
        affinity: Affinity::Integer,
        collation: Collation::Binary,
        hidden: true,
    });
    Scope { cols }
}

/// Whether every column reference in `e` is one the rows before source `k` can supply (or one from an outer
/// query), and there is no subquery.
fn available(e: &Expr, scope: &Scope, limit: usize) -> bool {
    let mut ok = true;
    visit(e, &mut |x| match x {
        Expr::Column { table, name } => {
            if let Ok(Some(p)) = scope.resolve(table.as_deref(), name)
                && p >= limit
            {
                ok = false;
            }
        }
        Expr::Subquery(_) | Expr::Exists(_) | Expr::InSelect { .. } => ok = false,
        _ => {}
    });
    ok
}

/// Calls `f` on `e` and every expression inside it (not into subqueries).
fn visit(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    match e {
        Expr::Unary(_, a)
        | Expr::IsNull { expr: a, .. }
        | Expr::Cast { expr: a, .. }
        | Expr::Collate { expr: a, .. } => visit(a, f),
        Expr::Binary(_, a, b) => {
            visit(a, f);
            visit(b, f);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            visit(expr, f);
            visit(low, f);
            visit(high, f);
        }
        Expr::InList { expr, list, .. } => {
            visit(expr, f);
            for x in list {
                visit(x, f);
            }
        }
        Expr::InSelect { expr, .. } => visit(expr, f),
        Expr::Like {
            expr,
            pattern,
            escape,
            ..
        } => {
            visit(expr, f);
            visit(pattern, f);
            if let Some(x) = escape {
                visit(x, f);
            }
        }
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            if let Some(o) = operand {
                visit(o, f);
            }
            for (w, t) in whens {
                visit(w, f);
                visit(t, f);
            }
            if let Some(o) = otherwise {
                visit(o, f);
            }
        }
        Expr::Function { args, .. } => args.iter().for_each(|x| visit(x, f)),
        Expr::Literal(_)
        | Expr::Param(_)
        | Expr::Column { .. }
        | Expr::Subquery(_)
        | Expr::Exists(_) => {}
    }
}

fn collect_aggregates(e: &Expr, out: &mut Vec<Expr>) {
    visit(e, &mut |x| {
        if let Expr::Function { name, args, .. } = x
            && ops::is_aggregate(name, args.len())
            && !out.contains(x)
        {
            out.push(x.clone());
        }
    });
}

/// Chooses how to read `table` as source `k`, given the conjuncts that could narrow it.
fn plan_access(table: &Table, offset: usize, scope: &Scope, preds: &[&Expr]) -> Access {
    let width = table.columns.len() + 1;
    let key_of = |e: &Expr| -> Option<ColKey> {
        let Expr::Column { table: t, name } = e else {
            return None;
        };
        let p = scope.resolve(t.as_deref(), name).ok().flatten()?;
        if p < offset || p >= offset + width {
            return None;
        }
        let c = p - offset;
        Some(
            if c == table.columns.len() || table.rowid_alias == Some(c) {
                ColKey::Rowid
            } else {
                ColKey::Col(c)
            },
        )
    };
    let mut eq: Vec<(ColKey, &Expr)> = Vec::new();
    let mut ranges: Vec<(ColKey, BinaryOp, &Expr)> = Vec::new();
    for p in preds {
        if let Expr::Between {
            expr,
            low,
            high,
            negated: false,
        } = p
            && let Some(k) = key_of(expr)
        {
            if available(low, scope, offset) {
                ranges.push((k, BinaryOp::Ge, &**low));
            }
            if available(high, scope, offset) {
                ranges.push((k, BinaryOp::Le, &**high));
            }
            continue;
        }
        let Expr::Binary(op, a, b) = p else { continue };
        if !matches!(
            op,
            BinaryOp::Eq | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) {
            continue;
        }
        let (key, other, op) = if let Some(k) = key_of(a) {
            (k, &**b, *op)
        } else if let Some(k) = key_of(b) {
            let flipped = match op {
                BinaryOp::Lt => BinaryOp::Gt,
                BinaryOp::Le => BinaryOp::Ge,
                BinaryOp::Gt => BinaryOp::Lt,
                BinaryOp::Ge => BinaryOp::Le,
                o => *o,
            };
            (k, &**a, flipped)
        } else {
            continue;
        };
        if !available(other, scope, offset) {
            continue;
        }
        if op == BinaryOp::Eq {
            eq.push((key, other));
        } else {
            ranges.push((key, op, other));
        }
    }
    if let Some((_, e)) = eq.iter().find(|(k, _)| *k == ColKey::Rowid) {
        return Access::RowidEq((*e).clone());
    }
    let bound = |key: ColKey, lower: bool| -> Bound {
        ranges
            .iter()
            .find(|(k, op, _)| {
                *k == key
                    && if lower {
                        matches!(op, BinaryOp::Gt | BinaryOp::Ge)
                    } else {
                        matches!(op, BinaryOp::Lt | BinaryOp::Le)
                    }
            })
            .map_or(Bound::None, |(_, op, e)| {
                Bound::At((*e).clone(), matches!(op, BinaryOp::Ge | BinaryOp::Le))
            })
    };
    let mut best: Option<(usize, Access)> = None;
    for (i, index) in table.indexes.iter().enumerate() {
        let mut exprs = Vec::new();
        for &c in &index.columns {
            match eq.iter().find(|(k, _)| *k == ColKey::Col(c)) {
                Some((_, e)) => exprs.push((*e).clone()),
                None => break,
            }
        }
        let next = index.columns.get(exprs.len()).map(|&c| ColKey::Col(c));
        let (lower, upper) = next.map_or((Bound::None, Bound::None), |k| {
            (bound(k, true), bound(k, false))
        });
        let has_range = !matches!(lower, Bound::None) || !matches!(upper, Bound::None);
        if exprs.is_empty() && !has_range {
            continue;
        }
        let mut score = exprs.len() * 10 + usize::from(has_range) * 4;
        if index.unique && exprs.len() == index.columns.len() {
            score += 1000;
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((
                score,
                Access::Index {
                    index: i,
                    eq: exprs,
                    lower,
                    upper,
                },
            ));
        }
    }
    let rowid_range = (bound(ColKey::Rowid, true), bound(ColKey::Rowid, false));
    let has_rowid_range =
        !matches!(rowid_range.0, Bound::None) || !matches!(rowid_range.1, Bound::None);
    match best {
        Some((score, access)) if score >= 10 || !has_rowid_range => access,
        _ if has_rowid_range => Access::RowidRange {
            lower: rowid_range.0,
            upper: rowid_range.1,
        },
        Some((_, access)) => access,
        None => Access::Full,
    }
}

fn int_of(v: &Value) -> Option<i64> {
    match value::to_number(v)? {
        Value::Integer(i) => Some(i),
        Value::Real(r) => Some(value::real_to_i64(r)),
        _ => None,
    }
}

impl<'a> Exec<'a> {
    // ---- scans

    fn prepare(&mut self, table: &Table, access: &Access, frames: &[Frame]) -> Result<Prepared> {
        let eval_key = |this: &mut Self, e: &Expr, aff: Affinity| -> Result<Value> {
            Ok(value::apply_affinity(this.eval(e, frames, None)?, aff))
        };
        Ok(match access {
            Access::Full => Prepared::Rowids { to: i64::MAX },
            Access::RowidEq(e) => {
                let v = self.eval(e, frames, None)?;
                match value::to_number(&v) {
                    // The rowid column has integer affinity: a text that is not a number matches nothing.
                    Some(Value::Integer(i))
                        if !matches!(v, Value::Text(_))
                            || value::parse_number(v.as_text().unwrap_or("")).is_some() =>
                    {
                        Prepared::Point(i)
                    }
                    Some(Value::Real(r)) if r.fract() == 0.0 && !matches!(v, Value::Text(_)) => {
                        Prepared::Point(value::real_to_i64(r))
                    }
                    _ => Prepared::Empty,
                }
            }
            Access::RowidRange { lower, upper } => {
                let mut from = i64::MIN;
                let mut to = i64::MAX;
                if let Bound::At(e, inclusive) = lower {
                    let v = self.eval(e, frames, None)?;
                    match int_of(&v) {
                        Some(i) => from = if *inclusive { i } else { i.saturating_add(1) },
                        None => return Ok(Prepared::Empty),
                    }
                }
                if let Bound::At(e, inclusive) = upper {
                    let v = self.eval(e, frames, None)?;
                    match int_of(&v) {
                        Some(i) => to = if *inclusive { i } else { i.saturating_sub(1) },
                        None => return Ok(Prepared::Empty),
                    }
                }
                if from > to {
                    return Ok(Prepared::Empty);
                }
                let _ = from;
                Prepared::Rowids { to }
            }
            Access::Index {
                index,
                eq,
                lower,
                upper,
            } => {
                let idx = &table.indexes[*index];
                let mut keys = Vec::new();
                for (e, &c) in eq.iter().zip(&idx.columns) {
                    let v = eval_key(self, e, table.columns[c].affinity)?;
                    if matches!(v, Value::Null) {
                        return Ok(Prepared::Empty);
                    }
                    keys.push(v);
                }
                let next_aff = idx
                    .columns
                    .get(eq.len())
                    .map_or(Affinity::Blob, |&c| table.columns[c].affinity);
                let mut bound = |b: &Bound| -> Result<Option<(Value, bool)>> {
                    match b {
                        Bound::None => Ok(None),
                        Bound::At(e, inclusive) => {
                            Ok(Some((eval_key(self, e, next_aff)?, *inclusive)))
                        }
                    }
                };
                let lo = bound(lower)?;
                let hi = bound(upper)?;
                if lo.as_ref().is_some_and(|(v, _)| matches!(v, Value::Null))
                    || hi.as_ref().is_some_and(|(v, _)| matches!(v, Value::Null))
                {
                    return Ok(Prepared::Empty);
                }
                Prepared::Index {
                    eq: keys,
                    lower: lo,
                    upper: hi,
                }
            }
        })
    }

    /// The next batch of rows of `table` along `access`; empty when there are no more.
    fn fetch(
        &mut self,
        table: &Table,
        access: &Access,
        frames: &[Frame],
        cursor: &mut Cursor,
    ) -> Result<Vec<(i64, Vec<Value>)>> {
        if cursor.done {
            return Ok(Vec::new());
        }
        if cursor.prepared.is_none() {
            let p = self.prepare(table, access, frames)?;
            // A range's lower end is where the first fetch starts.
            if let Access::RowidRange {
                lower: Bound::At(e, inclusive),
                ..
            } = access
                && !matches!(p, Prepared::Empty)
            {
                let v = self.eval(e, frames, None)?;
                if let Some(i) = int_of(&v) {
                    cursor.next_rowid = Some(if *inclusive { i } else { i.saturating_add(1) });
                }
            }
            cursor.prepared = Some(p);
        }
        let prepared = cursor.prepared.clone().unwrap_or(Prepared::Empty);
        let batch = cursor.batch.clamp(1, BATCH);
        cursor.batch = (batch * 2).min(BATCH);
        let mut out = Vec::new();
        match prepared {
            Prepared::Empty => cursor.done = true,
            Prepared::Point(rowid) => {
                cursor.done = true;
                let pager = self.db.pager_mut();
                if let Some(payload) = single_row(pager, table.root, rowid)? {
                    out.push((rowid, table.expand_row(rowid, record::decode(&payload)?)));
                }
            }
            Prepared::Rowids { to } => {
                let from = cursor.next_rowid.unwrap_or(i64::MIN);
                let mut raw: Vec<(i64, Vec<u8>)> = Vec::new();
                let mut finished = true;
                tree::table_scan_from(
                    self.db.pager_mut(),
                    table.root,
                    from,
                    &mut |rowid, payload| {
                        if rowid > to {
                            return Ok(false);
                        }
                        if raw.len() >= batch {
                            finished = false;
                            return Ok(false);
                        }
                        raw.push((rowid, payload));
                        Ok(true)
                    },
                )?;
                if finished {
                    cursor.done = true;
                }
                if let Some((last, _)) = raw.last() {
                    cursor.next_rowid = last.checked_add(1);
                    if cursor.next_rowid.is_none() {
                        cursor.done = true;
                    }
                }
                for (rowid, payload) in raw {
                    out.push((rowid, table.expand_row(rowid, record::decode(&payload)?)));
                }
            }
            Prepared::Index { eq, lower, upper } => {
                let Access::Index { index, .. } = access else {
                    return Err(Error::corrupt(0, "internal: index access without an index"));
                };
                let idx = &table.indexes[*index];
                let n = eq.len();
                let mut seek = eq.clone();
                if cursor.after.is_none()
                    && let Some((v, _)) = &lower
                {
                    seek.push(v.clone());
                }
                let start = cursor.after.clone().unwrap_or(seek);
                let skip_first = cursor.after.is_some();
                let mut entries: Vec<Vec<Value>> = Vec::new();
                let mut finished = true;
                let mut first = true;
                tree::index_scan_from(self.db.pager_mut(), idx.root, &start, &mut |entry| {
                    if skip_first && first {
                        first = false;
                        if tree::compare_entries(&entry, &start) == std::cmp::Ordering::Equal {
                            return Ok(true);
                        }
                    }
                    first = false;
                    if entry.len() <= n
                        || (0..n).any(|i| {
                            tree::compare_values(&entry[i], &eq[i]) != std::cmp::Ordering::Equal
                        })
                    {
                        return Ok(false);
                    }
                    if let Some((v, inclusive)) = &lower
                        && !*inclusive
                        && tree::compare_values(&entry[n], v) == std::cmp::Ordering::Equal
                    {
                        return Ok(true);
                    }
                    if let Some((v, inclusive)) = &upper {
                        let o = tree::compare_values(&entry[n], v);
                        if o == std::cmp::Ordering::Greater
                            || (o == std::cmp::Ordering::Equal && !*inclusive)
                        {
                            return Ok(false);
                        }
                    }
                    if entries.len() >= batch {
                        finished = false;
                        return Ok(false);
                    }
                    entries.push(entry);
                    Ok(true)
                })?;
                if finished {
                    cursor.done = true;
                }
                if let Some(last) = entries.last() {
                    cursor.after = Some(last.clone());
                }
                for entry in entries {
                    let Some(rowid) = entry.last().and_then(Value::as_integer) else {
                        return Err(Error::corrupt(idx.root, "an index entry without a rowid"));
                    };
                    let Some(payload) = single_row(self.db.pager_mut(), table.root, rowid)? else {
                        return Err(Error::corrupt(
                            idx.root,
                            "an index entry for a row that is not there",
                        ));
                    };
                    out.push((rowid, table.expand_row(rowid, record::decode(&payload)?)));
                }
            }
        }
        Ok(out)
    }

    // ---- SELECT

    /// Runs a query. `outer` are the rows of enclosing queries (for correlated subqueries).
    pub fn select(&mut self, sel: &Select, outer: &[Frame]) -> Result<Rows> {
        if sel.compounds.is_empty() {
            return self.select_core(
                &sel.core,
                &sel.order_by,
                sel.limit.as_ref(),
                sel.offset.as_ref(),
                outer,
            );
        }
        let mut result = self.select_core(&sel.core, &[], None, None, outer)?;
        for (op, core) in &sel.compounds {
            let next = self.select_core(core, &[], None, None, outer)?;
            if next.columns.len() != result.columns.len() {
                return Err(Error::Unsupported(
                    "SELECTs in a compound have different numbers of columns".into(),
                ));
            }
            result.rows = combine(*op, result.rows, next.rows);
        }
        if !sel.order_by.is_empty() {
            let keys: Vec<(usize, bool, NullsOrder)> = sel
                .order_by
                .iter()
                .map(|t| {
                    let idx = match &t.expr {
                        Expr::Literal(Value::Integer(n))
                            if *n >= 1 && (*n as usize) <= result.columns.len() =>
                        {
                            *n as usize - 1
                        }
                        Expr::Column { table: None, name } => result
                            .columns
                            .iter()
                            .position(|c| c.eq_ignore_ascii_case(name))
                            .ok_or_else(|| {
                                Error::Unsupported(format!(
                                    "ORDER BY {name}: not a column of the compound select"
                                ))
                            })?,
                        _ => {
                            return Err(Error::Unsupported(
                                "ORDER BY of a compound select must name a column".into(),
                            ));
                        }
                    };
                    Ok((idx, t.descending, t.nulls))
                })
                .collect::<Result<_>>()?;
            result.rows.sort_by(|a, b| {
                for &(i, desc, nulls) in &keys {
                    let o = null_aware(&a[i], &b[i], desc, nulls, Collation::Binary);
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        let (offset, limit) = self.limits(sel.limit.as_ref(), sel.offset.as_ref())?;
        result.rows = result
            .rows
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        Ok(result)
    }

    fn limits(
        &mut self,
        limit: Option<&Expr>,
        offset: Option<&Expr>,
    ) -> Result<(usize, Option<usize>)> {
        let mut eval = |e: Option<&Expr>| -> Result<Option<i64>> {
            match e {
                None => Ok(None),
                Some(e) => {
                    let v = self.eval(e, &[], None)?;
                    int_of(&v).map(Some).ok_or_else(|| {
                        Error::Unsupported("LIMIT and OFFSET must be integers".into())
                    })
                }
            }
        };
        let limit = eval(limit)?;
        let offset = eval(offset)?.unwrap_or(0).max(0) as usize;
        Ok((offset, limit.filter(|l| *l >= 0).map(|l| l as usize)))
    }

    fn build_sources(&mut self, from: &From, outer: &[Frame]) -> Result<(Vec<Source<'a>>, Scope)> {
        let catalog: &'a Catalog = self.catalog;
        let mut sources: Vec<Source<'a>> = Vec::new();
        let mut scope = Scope::default();
        let mut items: Vec<(&TableSource, bool, JoinConstraint)> =
            vec![(&from.first, false, JoinConstraint::None)];
        for j in &from.joins {
            items.push((&j.source, j.kind == JoinKind::Left, j.constraint.clone()));
        }
        for (src, left, constraint) in items {
            let offset = scope.cols.len();
            let (kind, part, alias) = match src {
                TableSource::Table { name, alias } => {
                    let table = catalog.table(name)?;
                    let a = alias.clone().unwrap_or_else(|| table.name.clone());
                    (SourceKind::Table(table), table_scope(table, &a), a)
                }
                TableSource::Subquery { select, alias } => {
                    let rows = self.select(select, outer)?;
                    let a = alias.clone().unwrap_or_default();
                    let part = Scope {
                        cols: rows
                            .columns
                            .iter()
                            .map(|c| ScopeCol {
                                table: a.to_ascii_lowercase(),
                                name: c.to_ascii_lowercase(),
                                label: c.clone(),
                                affinity: Affinity::Blob,
                                collation: Collation::Binary,
                                hidden: false,
                            })
                            .collect(),
                    };
                    (SourceKind::Derived(rows.rows), part, a)
                }
            };
            scope.cols.extend(part.cols);
            let mut merged = Vec::new();
            let on = match constraint {
                JoinConstraint::None => None,
                JoinConstraint::On(e) => Some(e),
                JoinConstraint::Using(cols) => {
                    let mut terms: Vec<Expr> = Vec::new();
                    for c in &cols {
                        let here = scope.cols[offset..]
                            .iter()
                            .position(|x| x.name == c.to_ascii_lowercase());
                        let before = scope.cols[..offset]
                            .iter()
                            .find(|x| x.name == c.to_ascii_lowercase());
                        let (Some(h), Some(b)) = (here, before) else {
                            return Err(Error::Unsupported(format!(
                                "USING column {c} is not in both tables"
                            )));
                        };
                        merged.push(h);
                        terms.push(Expr::Binary(
                            BinaryOp::Eq,
                            Box::new(Expr::Column {
                                table: Some(b.table.clone()),
                                name: c.clone(),
                            }),
                            Box::new(Expr::Column {
                                table: Some(scope.cols[offset + h].table.clone()),
                                name: c.clone(),
                            }),
                        ));
                    }
                    terms
                        .into_iter()
                        .reduce(|a, b| Expr::Binary(BinaryOp::And, Box::new(a), Box::new(b)))
                }
            };
            sources.push(Source {
                kind,
                offset,
                left,
                on,
                merged,
                alias,
            });
        }
        Ok((sources, scope))
    }

    #[allow(clippy::too_many_lines)]
    fn select_core(
        &mut self,
        core: &SelectCore,
        order_by: &[OrderTerm],
        limit: Option<&Expr>,
        offset: Option<&Expr>,
        outer: &[Frame],
    ) -> Result<Rows> {
        let (sources, scope) = match &core.from {
            Some(from) => self.build_sources(from, outer)?,
            None => (Vec::new(), Scope::default()),
        };
        let (offset_n, limit_n) = self.limits(limit, offset)?;

        // The output columns: names and expressions.
        let mut out_cols: Vec<(String, Expr)> = Vec::new();
        for rc in &core.columns {
            match rc {
                ResultColumn::All | ResultColumn::TableAll(_) => {
                    let wanted = if let ResultColumn::TableAll(t) = rc {
                        Some(t.to_ascii_lowercase())
                    } else {
                        None
                    };
                    let mut any = false;
                    for (si, s) in sources.iter().enumerate() {
                        if wanted
                            .as_ref()
                            .is_some_and(|w| *w != s.alias.to_ascii_lowercase())
                        {
                            continue;
                        }
                        let end = sources.get(si + 1).map_or(scope.cols.len(), |n| n.offset);
                        for (ci, c) in scope.cols[s.offset..end].iter().enumerate() {
                            if c.hidden || (wanted.is_none() && s.merged.contains(&ci)) {
                                continue;
                            }
                            any = true;
                            out_cols.push((
                                c.label.clone(),
                                Expr::Column {
                                    table: Some(c.table.clone()),
                                    name: c.label.clone(),
                                },
                            ));
                        }
                    }
                    if !any && sources.is_empty() {
                        return Err(Error::Unsupported("SELECT * with no tables".into()));
                    }
                    if !any && wanted.is_some() {
                        return Err(Error::Syntax {
                            pos: 0,
                            message: "no such table in the query".into(),
                        });
                    }
                }
                ResultColumn::Expr { expr, alias } => {
                    let name = alias.clone().unwrap_or_else(|| display_name(expr));
                    out_cols.push((name, expr.clone()));
                }
            }
        }

        // Aliases and ordinals in GROUP BY and ORDER BY.
        let resolve_ref = |e: &Expr| -> Expr {
            match e {
                Expr::Literal(Value::Integer(n)) if *n >= 1 && (*n as usize) <= out_cols.len() => {
                    out_cols[*n as usize - 1].1.clone()
                }
                Expr::Column { table: None, name } => out_cols
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map_or_else(|| e.clone(), |(_, x)| x.clone()),
                _ => e.clone(),
            }
        };
        let group_exprs: Vec<Expr> = core.group_by.iter().map(&resolve_ref).collect();
        let order_exprs: Vec<Expr> = order_by.iter().map(|t| resolve_ref(&t.expr)).collect();

        let mut calls: Vec<Expr> = Vec::new();
        for (_, e) in &out_cols {
            collect_aggregates(e, &mut calls);
        }
        if let Some(h) = &core.having {
            collect_aggregates(h, &mut calls);
        }
        for e in &order_exprs {
            collect_aggregates(e, &mut calls);
        }
        let grouped = !calls.is_empty() || !group_exprs.is_empty();

        // Early stop is only safe when nothing needs every row first.
        let need = (!grouped && order_by.is_empty() && !core.distinct && core.compounds_free())
            .then(|| limit_n.map(|l| l.saturating_add(offset_n)))
            .flatten();
        let rows = self.joined_rows(
            sources,
            scope.clone(),
            core.where_clause.as_ref(),
            outer,
            need,
        )?;

        let collations: Vec<Collation> = {
            let frames = [(&scope, &[][..])];
            order_exprs
                .iter()
                .map(|e| collation_of(e, &frames).unwrap_or_default())
                .collect()
        };
        let mut produced: Vec<(Vec<Value>, Vec<Value>)> = Vec::new();
        let width = scope.cols.len();
        if grouped {
            for group in self.groups(&rows, &group_exprs, &calls, &scope, outer, width)? {
                let frame_row: &[Value] = &group.row;
                let mut frames: Vec<Frame> = vec![(&scope, frame_row)];
                frames.extend_from_slice(outer);
                let aggs = Some(Aggregates {
                    calls: &calls,
                    values: &group.values,
                });
                if let Some(h) = &core.having
                    && value::truth(&self.eval(h, &frames, aggs)?) != Some(true)
                {
                    continue;
                }
                let mut out = Vec::with_capacity(out_cols.len());
                for (_, e) in &out_cols {
                    out.push(self.eval(e, &frames, aggs)?);
                }
                let mut keys = Vec::with_capacity(order_exprs.len());
                for e in &order_exprs {
                    keys.push(self.eval(e, &frames, aggs)?);
                }
                produced.push((out, keys));
            }
        } else {
            for row in &rows {
                let mut frames: Vec<Frame> = vec![(&scope, row)];
                frames.extend_from_slice(outer);
                let mut out = Vec::with_capacity(out_cols.len());
                for (_, e) in &out_cols {
                    out.push(self.eval(e, &frames, None)?);
                }
                let mut keys = Vec::with_capacity(order_exprs.len());
                for e in &order_exprs {
                    keys.push(self.eval(e, &frames, None)?);
                }
                produced.push((out, keys));
            }
        }

        if core.distinct {
            let mut seen = std::collections::BTreeSet::new();
            produced.retain(|(out, _)| seen.insert(group_key(out)));
        }
        if !order_by.is_empty() {
            produced.sort_by(|(_, a), (_, b)| {
                for (i, t) in order_by.iter().enumerate() {
                    let o = null_aware(&a[i], &b[i], t.descending, t.nulls, collations[i]);
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        let rows = produced
            .into_iter()
            .map(|(out, _)| out)
            .skip(offset_n)
            .take(limit_n.unwrap_or(usize::MAX))
            .collect();
        Ok(Rows {
            columns: out_cols.into_iter().map(|(n, _)| n).collect(),
            rows,
        })
    }

    /// Joins the sources and returns the rows that pass `WHERE`, each as the values of all the sources.
    fn joined_rows(
        &mut self,
        sources: Vec<Source<'a>>,
        scope: Scope,
        where_clause: Option<&Expr>,
        outer: &[Frame],
        need: Option<usize>,
    ) -> Result<Vec<Vec<Value>>> {
        let mut preds_all: Vec<&Expr> = Vec::new();
        if let Some(w) = where_clause {
            conjuncts(w, &mut preds_all);
        }
        let mut accesses = Vec::with_capacity(sources.len());
        for s in &sources {
            let SourceKind::Table(table) = &s.kind else {
                accesses.push(Access::Full);
                continue;
            };
            let mut preds: Vec<&Expr> = Vec::new();
            if let Some(on) = &s.on {
                conjuncts(on, &mut preds);
            }
            // A WHERE conjunct can narrow a source only if the source is not the inner side of a LEFT JOIN.
            if !s.left {
                preds.extend(preds_all.iter().copied());
            }
            accesses.push(plan_access(table, s.offset, &scope, &preds));
        }
        let width = scope.cols.len();
        let mut join = Join {
            sources,
            accesses,
            scope,
            where_clause,
            row: vec![Value::Null; width],
            out: Vec::new(),
            need,
        };
        self.level(&mut join, 0, outer)?;
        Ok(join.out)
    }

    /// Fills source `k` and everything after it. False when enough rows have been found.
    fn level(&mut self, j: &mut Join<'a, '_>, k: usize, outer: &[Frame]) -> Result<bool> {
        if k == j.sources.len() {
            if let Some(w) = j.where_clause {
                let mut frames: Vec<Frame> = vec![(&j.scope, &j.row)];
                frames.extend_from_slice(outer);
                if value::truth(&self.eval(w, &frames, None)?) != Some(true) {
                    return Ok(true);
                }
            }
            j.out.push(j.row.clone());
            return Ok(j.need.is_none_or(|n| j.out.len() < n));
        }
        let offset = j.sources[k].offset;
        let left = j.sources[k].left;
        let on = j.sources[k].on.clone();
        let mut matched = false;

        // Candidate rows of this source: (hidden rowid or None, values).
        let mut try_row = |this: &mut Self,
                           j: &mut Join<'a, '_>,
                           rowid: Option<i64>,
                           values: &[Value]|
         -> Result<bool> {
            j.row[offset..offset + values.len()].clone_from_slice(values);
            if let Some(r) = rowid {
                j.row[offset + values.len()] = Value::Integer(r);
            }
            if let Some(on) = &on {
                let mut frames: Vec<Frame> = vec![(&j.scope, &j.row)];
                frames.extend_from_slice(outer);
                if value::truth(&this.eval(on, &frames, None)?) != Some(true) {
                    return Ok(true);
                }
            }
            matched = true;
            this.level(j, k + 1, outer)
        };

        let mut keep_going = true;
        match &j.sources[k].kind {
            SourceKind::Derived(rows) => {
                let rows = rows.clone();
                for values in &rows {
                    if !try_row(self, j, None, values)? {
                        keep_going = false;
                        break;
                    }
                }
            }
            SourceKind::Table(table) => {
                let table: &Table = table;
                let access = j.accesses[k].clone();
                let mut cursor = Cursor {
                    batch: j.need.map_or(BATCH, |n| n.max(1)),
                    ..Cursor::default()
                };
                'batches: loop {
                    let batch = {
                        let mut frames: Vec<Frame> = vec![(&j.scope, &j.row)];
                        frames.extend_from_slice(outer);
                        self.fetch(table, &access, &frames, &mut cursor)?
                    };
                    if batch.is_empty() {
                        break;
                    }
                    for (rowid, values) in batch {
                        if !try_row(self, j, Some(rowid), &values)? {
                            keep_going = false;
                            break 'batches;
                        }
                    }
                }
            }
        }
        if keep_going && left && !matched {
            let width = j
                .sources
                .get(k + 1)
                .map_or(j.scope.cols.len(), |n| n.offset)
                - offset;
            for v in &mut j.row[offset..offset + width] {
                *v = Value::Null;
            }
            keep_going = self.level(j, k + 1, outer)?;
        }
        Ok(keep_going)
    }

    fn groups(
        &mut self,
        rows: &[Vec<Value>],
        group_exprs: &[Expr],
        calls: &[Expr],
        scope: &Scope,
        outer: &[Frame],
        width: usize,
    ) -> Result<Vec<Group>> {
        let mut index: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
        let mut members: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let mut frames: Vec<Frame> = vec![(scope, row)];
            frames.extend_from_slice(outer);
            let mut key = Vec::with_capacity(group_exprs.len());
            for e in group_exprs {
                key.push(self.eval(e, &frames, None)?);
            }
            let bytes = group_key(&key);
            let at = *index.entry(bytes).or_insert_with(|| {
                members.push((key, Vec::new()));
                members.len() - 1
            });
            members[at].1.push(r);
        }
        if group_exprs.is_empty() && members.is_empty() {
            members.push((Vec::new(), Vec::new()));
        }
        // Groups come out in the order of their keys.
        members.sort_by(|a, b| tree::compare_entries(&a.0, &b.0));
        let mut groups = Vec::with_capacity(members.len());
        for (_, member_rows) in members {
            let mut values = Vec::with_capacity(calls.len());
            for call in calls {
                let Expr::Function {
                    name,
                    args,
                    distinct,
                    star,
                } = call
                else {
                    return Err(Error::corrupt(
                        0,
                        "internal: an aggregate that is not a call",
                    ));
                };
                let mut acc = Accumulator::new(name)?;
                let mut seen = std::collections::BTreeSet::new();
                let collation = args
                    .first()
                    .and_then(|a| collation_of(a, &[(scope, &[][..])]))
                    .unwrap_or_default();
                for &r in &member_rows {
                    let mut frames: Vec<Frame> = vec![(scope, &rows[r])];
                    frames.extend_from_slice(outer);
                    let arg = match args.first() {
                        Some(a) if !*star => Some(self.eval(a, &frames, None)?),
                        _ => None,
                    };
                    if *distinct
                        && let Some(v) = &arg
                        && (matches!(v, Value::Null)
                            || !seen.insert(group_key(std::slice::from_ref(v))))
                    {
                        continue;
                    }
                    let separator = match args.get(1) {
                        Some(a) => Some(self.eval(a, &frames, None)?),
                        None => None,
                    };
                    acc.step(arg.as_ref(), separator.as_ref(), collation);
                }
                values.push(acc.finish());
            }
            let row = member_rows
                .first()
                .map_or_else(|| vec![Value::Null; width], |&r| rows[r].clone());
            groups.push(Group { row, values });
        }
        Ok(groups)
    }
}

struct Group {
    row: Vec<Value>,
    values: Vec<Value>,
}

trait CoreExt {
    fn compounds_free(&self) -> bool;
}

impl CoreExt for SelectCore {
    fn compounds_free(&self) -> bool {
        true
    }
}

fn single_row(pager: &mut crate::pager::Pager, root: u32, rowid: i64) -> Result<Option<Vec<u8>>> {
    let mut found = None;
    tree::table_scan_from(pager, root, rowid, &mut |r, payload| {
        if r == rowid {
            found = Some(payload);
        }
        Ok(false)
    })?;
    Ok(found)
}

/// The key two values group (or are distinct) by: 1 and 1.0 are the same, a text is not a number.
fn group_key(values: &[Value]) -> Vec<u8> {
    let normalized: Vec<Value> = values
        .iter()
        .map(|v| match v {
            Value::Real(r) if r.fract() == 0.0 && r.abs() < 9.0e18 => Value::Integer(*r as i64),
            other => other.clone(),
        })
        .collect();
    record::encode(&normalized)
}

fn null_aware(
    a: &Value,
    b: &Value,
    descending: bool,
    nulls: NullsOrder,
    collation: Collation,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (an, bn) = (matches!(a, Value::Null), matches!(b, Value::Null));
    if an || bn {
        // NULL is the smallest value: first when ascending, last when descending, unless asked otherwise.
        let null_first = match nulls {
            NullsOrder::First => true,
            NullsOrder::Last => false,
            NullsOrder::Default => !descending,
        };
        return match (an, bn) {
            (true, true) => Ordering::Equal,
            (true, false) => {
                if null_first {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            _ => {
                if null_first {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        };
    }
    let o = order_compare(a, b, collation);
    if descending { o.reverse() } else { o }
}

fn combine(op: CompoundOp, left: Vec<Vec<Value>>, right: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    use std::collections::BTreeSet;
    match op {
        CompoundOp::UnionAll => left.into_iter().chain(right).collect(),
        CompoundOp::Union => {
            let mut seen = BTreeSet::new();
            left.into_iter()
                .chain(right)
                .filter(|r| seen.insert(group_key(r)))
                .collect()
        }
        CompoundOp::Intersect => {
            let keep: BTreeSet<Vec<u8>> = right.iter().map(|r| group_key(r)).collect();
            let mut seen = BTreeSet::new();
            left.into_iter()
                .filter(|r| keep.contains(&group_key(r)) && seen.insert(group_key(r)))
                .collect()
        }
        CompoundOp::Except => {
            let drop: BTreeSet<Vec<u8>> = right.iter().map(|r| group_key(r)).collect();
            let mut seen = BTreeSet::new();
            left.into_iter()
                .filter(|r| !drop.contains(&group_key(r)) && seen.insert(group_key(r)))
                .collect()
        }
    }
}

/// A short name for an expression that has no alias (what a result column is called).
pub fn display_name(e: &Expr) -> String {
    match e {
        Expr::Column { name, .. } => name.clone(),
        Expr::Literal(Value::Integer(i)) => i.to_string(),
        Expr::Literal(Value::Real(r)) => value::format_real(*r),
        Expr::Literal(Value::Text(s)) => s.clone(),
        Expr::Literal(Value::Null) => "NULL".into(),
        Expr::Function {
            name, args, star, ..
        } => {
            let inner = if *star {
                "*".to_string()
            } else {
                args.iter().map(display_name).collect::<Vec<_>>().join(",")
            };
            format!("{name}({inner})")
        }
        Expr::Binary(op, a, b) => {
            let sym = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Concat => "||",
                BinaryOp::Eq => "=",
                _ => "?",
            };
            format!("{}{sym}{}", display_name(a), display_name(b))
        }
        _ => "expr".into(),
    }
}

impl<'a> Exec<'a> {
    /// The rows of `table` that pass `where_clause`, with their rowids (for `UPDATE`, `DELETE` and indexes).
    pub fn table_rows(
        &mut self,
        table: &'a Table,
        alias: &str,
        where_clause: Option<&Expr>,
        outer: &[Frame],
    ) -> Result<Vec<(i64, Vec<Value>)>> {
        let scope = table_scope(table, alias);
        let source = Source {
            kind: SourceKind::Table(table),
            offset: 0,
            left: false,
            on: None,
            merged: Vec::new(),
            alias: alias.to_string(),
        };
        let n = table.columns.len();
        let rows = self.joined_rows(vec![source], scope, where_clause, outer, None)?;
        Ok(rows
            .into_iter()
            .map(|mut row| {
                let rowid = row.pop().and_then(|v| v.as_integer()).unwrap_or(0);
                (rowid, row[..n].to_vec())
            })
            .collect())
    }

    /// The scope of a table under a name, for evaluating its constraints.
    #[must_use]
    pub fn scope_of(table: &Table, alias: &str) -> Scope {
        table_scope(table, alias)
    }
}
