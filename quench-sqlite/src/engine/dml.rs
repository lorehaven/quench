//! `INSERT`, `UPDATE` and `DELETE`: constraints, conflicts, upserts, and keeping the indexes in step.

use super::catalog::{Index, Table};
use super::eval::{Aggregates, Frame, Scope, ScopeCol};
use super::exec::{Exec, Rows, display_name};
use super::value::{self, Affinity, Collation};
use crate::error::{Error, Result};
use crate::record::{self, Value};
use crate::sql::ast::{
    Delete, Insert, InsertSource, OnConflict, ResultColumn, Update, Upsert, UpsertAction,
};
use crate::tree;

/// A row in the way: its rowid and what it conflicts on.
struct Conflict {
    rowid: i64,
    /// The columns of the broken constraint, as `table.column` words.
    what: String,
    /// Which index (None: the rowid).
    index: Option<usize>,
}

fn entry_of(index: &Index, values: &[Value], rowid: i64) -> Vec<Value> {
    let mut e: Vec<Value> = index.columns.iter().map(|&c| values[c].clone()).collect();
    e.push(Value::Integer(rowid));
    e
}

fn stored_record(table: &Table, values: &[Value]) -> Vec<u8> {
    let mut v = values.to_vec();
    if let Some(a) = table.rowid_alias {
        v[a] = Value::Null;
    }
    record::encode(&v)
}

impl<'a> Exec<'a> {
    // ---- writing one row

    fn add_row(&mut self, table: &Table, rowid: i64, values: &[Value]) -> Result<()> {
        let pager = self.db.pager_mut();
        tree::table_insert(
            pager,
            table.root,
            rowid,
            &stored_record(table, values),
            false,
        )?;
        for index in &table.indexes {
            tree::index_insert(pager, index.root, &entry_of(index, values, rowid))?;
        }
        Ok(())
    }

    fn remove_row(&mut self, table: &Table, rowid: i64, values: &[Value]) -> Result<()> {
        let pager = self.db.pager_mut();
        for index in &table.indexes {
            tree::index_delete(pager, index.root, &entry_of(index, values, rowid))?;
        }
        tree::table_delete(pager, table.root, rowid)?;
        Ok(())
    }

    /// Applies affinity, the rowid alias, `NOT NULL` and `CHECK`. `Ok(None)`: the row is skipped (`OR IGNORE`).
    fn check_row(
        &mut self,
        table: &Table,
        values: &mut [Value],
        or: OnConflict,
    ) -> Result<Option<()>> {
        for (i, column) in table.columns.iter().enumerate() {
            values[i] = value::apply_affinity(
                std::mem::replace(&mut values[i], Value::Null),
                column.affinity,
            );
            if matches!(values[i], Value::Null) && column.not_null && table.rowid_alias != Some(i) {
                if or == OnConflict::Replace
                    && let Some(d) = &column.default
                {
                    values[i] = self.eval(d, &[], None)?;
                } else if or == OnConflict::Ignore {
                    return Ok(None);
                } else {
                    return Err(Error::Constraint(format!(
                        "NOT NULL constraint failed: {}.{}",
                        table.name, column.name
                    )));
                }
            }
        }
        if !table.checks.is_empty() {
            let scope = Self::scope_of(table, &table.name);
            for check in &table.checks {
                let frames: [Frame; 1] = [(&scope, values)];
                // A CHECK passes unless it is false; NULL passes.
                if value::truth(&self.eval(check, &frames, None)?) == Some(false) {
                    return Err(Error::Constraint(format!(
                        "CHECK constraint failed: {}",
                        table.name
                    )));
                }
            }
        }
        Ok(Some(()))
    }

    /// Rows that `values` (to be stored under `rowid`) would collide with, `own` excepted.
    fn conflicts(
        &mut self,
        table: &Table,
        rowid: i64,
        values: &[Value],
        own: Option<i64>,
    ) -> Result<Vec<Conflict>> {
        let mut found = Vec::new();
        if own != Some(rowid) {
            let mut exists = false;
            tree::table_scan_from(self.db.pager_mut(), table.root, rowid, &mut |r, _| {
                exists = r == rowid;
                Ok(false)
            })?;
            if exists {
                let name = table
                    .rowid_alias
                    .map_or("rowid", |a| table.columns[a].name.as_str());
                found.push(Conflict {
                    rowid,
                    what: format!("{}.{name}", table.name),
                    index: None,
                });
            }
        }
        for (i, index) in table.indexes.iter().enumerate().filter(|(_, x)| x.unique) {
            let key: Vec<Value> = index.columns.iter().map(|&c| values[c].clone()).collect();
            if key.iter().any(|v| matches!(v, Value::Null)) {
                continue;
            }
            let mut hit = None;
            tree::index_scan_from(self.db.pager_mut(), index.root, &key, &mut |entry| {
                let same = entry.len() == key.len() + 1
                    && entry
                        .iter()
                        .zip(&key)
                        .all(|(a, b)| tree::compare_values(a, b) == std::cmp::Ordering::Equal);
                if same {
                    hit = entry.last().and_then(Value::as_integer);
                }
                Ok(false)
            })?;
            if let Some(other) = hit.filter(|o| Some(*o) != own) {
                let what = index
                    .columns
                    .iter()
                    .map(|&c| format!("{}.{}", table.name, table.columns[c].name))
                    .collect::<Vec<_>>()
                    .join(", ");
                found.push(Conflict {
                    rowid: other,
                    what,
                    index: Some(i),
                });
            }
        }
        Ok(found)
    }

    fn returning(
        &mut self,
        table: &Table,
        cols: &[ResultColumn],
        values: &[Value],
        rowid: i64,
        out: &mut Rows,
    ) -> Result<()> {
        if cols.is_empty() {
            return Ok(());
        }
        let scope = Self::scope_of(table, &table.name);
        let mut row = values.to_vec();
        row.push(Value::Integer(rowid));
        let frames: [Frame; 1] = [(&scope, &row)];
        let first = out.columns.is_empty();
        let mut line = Vec::new();
        for c in cols {
            match c {
                ResultColumn::All | ResultColumn::TableAll(_) => {
                    for (i, col) in table.columns.iter().enumerate() {
                        if first {
                            out.columns.push(col.name.clone());
                        }
                        line.push(values[i].clone());
                    }
                }
                ResultColumn::Expr { expr, alias } => {
                    if first {
                        out.columns
                            .push(alias.clone().unwrap_or_else(|| display_name(expr)));
                    }
                    line.push(self.eval(expr, &frames, None)?);
                }
            }
        }
        out.rows.push(line);
        Ok(())
    }

    // ---- INSERT

    /// Runs an `INSERT`; the rows of `RETURNING`, if it has any.
    pub fn insert(&mut self, ins: &Insert, outer: &[Frame]) -> Result<Rows> {
        let catalog = self.catalog;
        let table: &'a Table = catalog.table(&ins.table)?;
        let positions: Vec<usize> = match &ins.columns {
            Some(names) => names
                .iter()
                .map(|n| {
                    table.column(n).ok_or_else(|| Error::Syntax {
                        pos: 0,
                        message: format!("table {} has no column named {n}", table.name),
                    })
                })
                .collect::<Result<_>>()?,
            None => (0..table.columns.len()).collect(),
        };
        let source_rows: Vec<Vec<Value>> = match &ins.source {
            InsertSource::DefaultValues => vec![Vec::new()],
            InsertSource::Values(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut vals = Vec::with_capacity(row.len());
                    for e in row {
                        vals.push(self.eval(e, outer, None)?);
                    }
                    out.push(vals);
                }
                out
            }
            InsertSource::Select(s) => self.select(s, outer)?.rows,
        };
        let mut result = Rows::default();
        for given in source_rows {
            if !matches!(ins.source, InsertSource::DefaultValues) && given.len() != positions.len()
            {
                return Err(Error::Syntax {
                    pos: 0,
                    message: format!("{} values for {} columns", given.len(), positions.len()),
                });
            }
            let mut values = vec![Value::Null; table.columns.len()];
            let mut set = vec![false; table.columns.len()];
            for (v, &p) in given.into_iter().zip(&positions) {
                values[p] = v;
                set[p] = true;
            }
            for (i, c) in table.columns.iter().enumerate() {
                if !set[i]
                    && let Some(d) = &c.default
                {
                    values[i] = self.eval(d, &[], None)?;
                }
            }
            self.insert_one(table, ins, values, &mut result)?;
        }
        Ok(result)
    }

    fn insert_one(
        &mut self,
        table: &'a Table,
        ins: &Insert,
        mut values: Vec<Value>,
        result: &mut Rows,
    ) -> Result<()> {
        // The rowid: the alias column if it has a value, else one more than the largest.
        let given_rowid = match table.rowid_alias.map(|a| &values[a]) {
            Some(Value::Null) | None => None,
            Some(v) => Some(match value::apply_affinity(v.clone(), Affinity::Integer) {
                Value::Integer(i) => i,
                _ => return Err(Error::Constraint("datatype mismatch".into())),
            }),
        };
        if let Some(a) = table.rowid_alias
            && let Some(r) = given_rowid
        {
            values[a] = Value::Integer(r);
        }
        let or = if ins.upsert.is_some() {
            OnConflict::Abort
        } else {
            ins.or
        };
        // `check_row` before the rowid is known: the alias column is not checked for NULL.
        if self.check_row(table, &mut values, or)?.is_none() {
            return Ok(());
        }
        let rowid = match given_rowid {
            Some(r) => r,
            None => tree::next_rowid(self.db.pager_mut(), table.root)?,
        };
        if let Some(a) = table.rowid_alias {
            values[a] = Value::Integer(rowid);
        }
        let conflicts = self.conflicts(table, rowid, &values, None)?;
        if !conflicts.is_empty() {
            if let Some(up) = &ins.upsert {
                return self.upsert(table, up, &conflicts, &values, ins, result);
            }
            match ins.or {
                OnConflict::Ignore => return Ok(()),
                OnConflict::Replace => {
                    let mut gone = Vec::new();
                    for c in &conflicts {
                        if gone.contains(&c.rowid) {
                            continue;
                        }
                        gone.push(c.rowid);
                        let old = self.row_by_id(table, c.rowid)?;
                        self.remove_row(table, c.rowid, &old)?;
                    }
                }
                _ => {
                    let c = &conflicts[0];
                    return Err(Error::Constraint(format!(
                        "UNIQUE constraint failed: {}",
                        c.what
                    )));
                }
            }
        }
        self.add_row(table, rowid, &values)?;
        self.last_rowid = rowid;
        self.changes += 1;
        self.returning(table, &ins.returning, &values, rowid, result)
    }

    fn row_by_id(&mut self, table: &Table, rowid: i64) -> Result<Vec<Value>> {
        let mut payload = None;
        tree::table_scan_from(self.db.pager_mut(), table.root, rowid, &mut |r, p| {
            if r == rowid {
                payload = Some(p);
            }
            Ok(false)
        })?;
        let payload =
            payload.ok_or_else(|| Error::corrupt(table.root, "a row that was found is gone"))?;
        Ok(table.expand_row(rowid, record::decode(&payload)?))
    }

    fn upsert(
        &mut self,
        table: &'a Table,
        up: &Upsert,
        conflicts: &[Conflict],
        proposed: &[Value],
        ins: &Insert,
        result: &mut Rows,
    ) -> Result<()> {
        let target_matches = |c: &Conflict| -> bool {
            if up.target.is_empty() {
                return true;
            }
            let wanted: Vec<usize> = up.target.iter().filter_map(|n| table.column(n)).collect();
            match c.index {
                None => table.rowid_alias.is_some_and(|a| wanted == [a]),
                Some(i) => {
                    let mut have = table.indexes[i].columns.clone();
                    let mut want = wanted;
                    have.sort_unstable();
                    want.sort_unstable();
                    have == want
                }
            }
        };
        let Some(hit) = conflicts.iter().find(|c| target_matches(c)) else {
            let c = &conflicts[0];
            return Err(Error::Constraint(format!(
                "UNIQUE constraint failed: {}",
                c.what
            )));
        };
        let UpsertAction::Update { sets, where_clause } = &up.action else {
            return Ok(());
        };
        let old = self.row_by_id(table, hit.rowid)?;
        let scope = Self::scope_of(table, &table.name);
        let excluded = Scope {
            cols: table
                .columns
                .iter()
                .map(|c| ScopeCol {
                    table: "excluded".into(),
                    name: c.name.to_ascii_lowercase(),
                    label: c.name.clone(),
                    affinity: c.affinity,
                    collation: Collation::Binary,
                    hidden: false,
                })
                .collect(),
        };
        let mut old_row = old.clone();
        old_row.push(Value::Integer(hit.rowid));
        let frames: [Frame; 2] = [(&scope, &old_row), (&excluded, proposed)];
        if let Some(w) = where_clause
            && value::truth(&self.eval(w, &frames, None)?) != Some(true)
        {
            return Ok(());
        }
        let mut new = old.clone();
        for (name, e) in sets {
            let i = table.column(name).ok_or_else(|| Error::Syntax {
                pos: 0,
                message: format!("no such column: {name}"),
            })?;
            new[i] = self.eval(e, &frames, None)?;
        }
        self.replace_checked(table, hit.rowid, &old, new.clone(), OnConflict::Abort)?;
        self.changes += 1;
        let new_rowid = table
            .rowid_alias
            .and_then(|a| new[a].as_integer())
            .unwrap_or(hit.rowid);
        self.returning(table, &ins.returning, &new, new_rowid, result)
    }

    // ---- UPDATE, DELETE

    /// Changes one row (possibly its rowid), checking constraints against everything but itself.
    fn replace_checked(
        &mut self,
        table: &Table,
        rowid: i64,
        old: &[Value],
        mut new: Vec<Value>,
        or: OnConflict,
    ) -> Result<bool> {
        if self.check_row(table, &mut new, or)?.is_none() {
            return Ok(false);
        }
        let new_rowid = match table.rowid_alias {
            Some(a) => match &new[a] {
                Value::Integer(i) => *i,
                Value::Null => rowid,
                _ => return Err(Error::Constraint("datatype mismatch".into())),
            },
            None => rowid,
        };
        if let Some(a) = table.rowid_alias {
            new[a] = Value::Integer(new_rowid);
        }
        let conflicts = self.conflicts(table, new_rowid, &new, Some(rowid))?;
        if !conflicts.is_empty() {
            match or {
                OnConflict::Ignore => return Ok(false),
                OnConflict::Replace => {
                    for c in &conflicts {
                        let gone = self.row_by_id(table, c.rowid)?;
                        self.remove_row(table, c.rowid, &gone)?;
                    }
                }
                _ => {
                    return Err(Error::Constraint(format!(
                        "UNIQUE constraint failed: {}",
                        conflicts[0].what
                    )));
                }
            }
        }
        let pager = self.db.pager_mut();
        for index in &table.indexes {
            let (a, b) = (
                entry_of(index, old, rowid),
                entry_of(index, &new, new_rowid),
            );
            if a != b {
                tree::index_delete(pager, index.root, &a)?;
                tree::index_insert(pager, index.root, &b)?;
            }
        }
        if new_rowid == rowid {
            tree::table_insert(pager, table.root, rowid, &stored_record(table, &new), true)?;
        } else {
            tree::table_delete(pager, table.root, rowid)?;
            tree::table_insert(
                pager,
                table.root,
                new_rowid,
                &stored_record(table, &new),
                false,
            )?;
        }
        Ok(true)
    }

    /// Runs an `UPDATE`.
    pub fn update(&mut self, upd: &Update, outer: &[Frame]) -> Result<Rows> {
        let catalog = self.catalog;
        let table: &'a Table = catalog.table(&upd.table)?;
        let targets: Vec<usize> = upd
            .sets
            .iter()
            .map(|(n, _)| {
                table.column(n).ok_or_else(|| Error::Syntax {
                    pos: 0,
                    message: format!("no such column: {n}"),
                })
            })
            .collect::<Result<_>>()?;
        let rows = self.table_rows(table, &table.name, upd.where_clause.as_ref(), outer)?;
        let scope = Self::scope_of(table, &table.name);
        let mut result = Rows::default();
        for (rowid, old) in rows {
            let mut full = old.clone();
            full.push(Value::Integer(rowid));
            let mut new = old.clone();
            {
                let mut frames: Vec<Frame> = vec![(&scope, &full)];
                frames.extend_from_slice(outer);
                for ((_, e), &i) in upd.sets.iter().zip(&targets) {
                    new[i] = self.eval(e, &frames, None)?;
                }
            }
            if self.replace_checked(table, rowid, &old, new.clone(), upd.or)? {
                self.changes += 1;
                let new_rowid = table
                    .rowid_alias
                    .and_then(|a| new[a].as_integer())
                    .unwrap_or(rowid);
                self.returning(table, &upd.returning, &new, new_rowid, &mut result)?;
            }
        }
        Ok(result)
    }

    /// Runs a `DELETE`.
    pub fn delete(&mut self, del: &Delete, outer: &[Frame]) -> Result<Rows> {
        let catalog = self.catalog;
        let table: &'a Table = catalog.table(&del.table)?;
        let rows = self.table_rows(table, &table.name, del.where_clause.as_ref(), outer)?;
        let mut result = Rows::default();
        for (rowid, values) in rows {
            self.returning(table, &del.returning, &values, rowid, &mut result)?;
            self.remove_row(table, rowid, &values)?;
            self.changes += 1;
        }
        Ok(result)
    }

    /// Adds an entry to every row's place in a new index.
    pub fn fill_index(&mut self, table: &'a Table, index: &Index) -> Result<()> {
        let rows = self.table_rows(table, &table.name, None, &[])?;
        for (rowid, values) in rows {
            let entry = entry_of(index, &values, rowid);
            if index.unique
                && entry[..entry.len() - 1]
                    .iter()
                    .all(|v| !matches!(v, Value::Null))
            {
                let key = &entry[..entry.len() - 1];
                let mut dup = false;
                tree::index_scan_from(self.db.pager_mut(), index.root, key, &mut |e| {
                    dup = e.len() == key.len() + 1
                        && e.iter()
                            .zip(key)
                            .all(|(a, b)| tree::compare_values(a, b) == std::cmp::Ordering::Equal);
                    Ok(false)
                })?;
                if dup {
                    let names = index
                        .columns
                        .iter()
                        .map(|&c| format!("{}.{}", table.name, table.columns[c].name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(Error::Constraint(format!(
                        "UNIQUE constraint failed: {names}"
                    )));
                }
            }
            tree::index_insert(self.db.pager_mut(), index.root, &entry)?;
        }
        Ok(())
    }
}

#[allow(dead_code)]
const fn unused(_: Option<Aggregates>) {}
