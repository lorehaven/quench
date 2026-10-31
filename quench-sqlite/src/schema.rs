//! `sqlite_schema`, the table on page 1 that lists every table, index, view and trigger.

use crate::btree;
use crate::error::{Error, Result};
use crate::pager::Pager;
use crate::record::{self, Value};
use crate::tree::{self, Tree};

/// What a schema row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Table,
    Index,
    View,
    Trigger,
}

/// One row of `sqlite_schema`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    pub kind: ObjectKind,
    pub name: String,
    /// The table an index or trigger belongs to (the object's own name for a table).
    pub table: String,
    /// The root page of a table or index, 0 for views and triggers.
    pub root_page: u32,
    /// The `CREATE` statement; `None` for the automatic indexes of `UNIQUE` and `PRIMARY KEY`.
    pub sql: Option<String>,
}

/// Reads every object of the database.
pub fn read(pager: &mut Pager) -> Result<Vec<Object>> {
    let mut objects = Vec::new();
    btree::scan_table(pager, 1, &mut |_, payload| {
        let row = record::decode(payload)?;
        objects.push(object(&row)?);
        Ok(())
    })?;
    Ok(objects)
}

fn object(row: &[Value]) -> Result<Object> {
    let bad = || {
        Error::corrupt(
            1,
            "a row of sqlite_schema is not (type, name, tbl_name, rootpage, sql)",
        )
    };
    let [
        Value::Text(kind),
        Value::Text(name),
        Value::Text(table),
        root,
        sql,
        ..,
    ] = row
    else {
        return Err(bad());
    };
    let kind = match kind.as_str() {
        "table" => ObjectKind::Table,
        "index" => ObjectKind::Index,
        "view" => ObjectKind::View,
        "trigger" => ObjectKind::Trigger,
        _ => return Err(bad()),
    };
    let root_page = match root {
        Value::Integer(n) => u32::try_from(*n).map_err(|_| bad())?,
        Value::Null => 0,
        _ => return Err(bad()),
    };
    let sql = match sql {
        Value::Text(s) => Some(s.clone()),
        _ => None,
    };
    Ok(Object {
        kind,
        name: name.clone(),
        table: table.clone(),
        root_page,
        sql,
    })
}

/// Adds a table or index to the schema: a new empty tree and its row in `sqlite_schema`. Returns the root page.
/// `sql` is stored as given; checking it is for the layer above.
pub fn create(
    pager: &mut Pager,
    kind: ObjectKind,
    name: &str,
    table: &str,
    sql: Option<&str>,
) -> Result<u32> {
    let (word, tree) = match kind {
        ObjectKind::Table => ("table", Tree::Table),
        ObjectKind::Index => ("index", Tree::Index),
        _ => {
            return Err(Error::Unsupported(
                "only tables and indexes are created here".into(),
            ));
        }
    };
    if find(pager, name)?.is_some() {
        return Err(Error::KeyExists);
    }
    let root = tree::new_tree(pager, tree)?;
    let rowid = tree::next_rowid(pager, 1)?;
    let row = record::encode(&[
        Value::Text(word.into()),
        Value::Text(name.into()),
        Value::Text(table.into()),
        Value::Integer(i64::from(root)),
        sql.map_or(Value::Null, |s| Value::Text(s.into())),
    ]);
    tree::table_insert(pager, 1, rowid, &row, false)?;
    bump_cookie(pager)?;
    Ok(root)
}

/// Removes a table or index from the schema and frees its pages. False when there is no such object.
pub fn drop(pager: &mut Pager, name: &str) -> Result<bool> {
    let Some((rowid, object)) = find(pager, name)? else {
        return Ok(false);
    };
    tree::table_delete(pager, 1, rowid)?;
    if object.root_page != 0 {
        tree::free_tree(pager, object.root_page)?;
    }
    bump_cookie(pager)?;
    Ok(true)
}

fn find(pager: &mut Pager, name: &str) -> Result<Option<(i64, Object)>> {
    let mut found = None;
    btree::scan_table(pager, 1, &mut |rowid, payload| {
        let obj = object(&record::decode(payload)?)?;
        if found.is_none() && obj.name.eq_ignore_ascii_case(name) {
            found = Some((rowid, obj));
        }
        Ok(())
    })?;
    Ok(found)
}

fn bump_cookie(pager: &mut Pager) -> Result<()> {
    let header = pager.header_mut()?;
    header.schema_cookie = header.schema_cookie.wrapping_add(1);
    Ok(())
}

/// Replaces the `CREATE` text of a table (for `ALTER TABLE ... ADD COLUMN`).
pub fn set_sql(pager: &mut Pager, name: &str, sql: &str) -> Result<()> {
    let Some((rowid, object)) = find(pager, name)? else {
        return Err(Error::corrupt(
            1,
            "the object to change is not in the schema",
        ));
    };
    let kind = match object.kind {
        ObjectKind::Table => "table",
        ObjectKind::Index => "index",
        ObjectKind::View => "view",
        ObjectKind::Trigger => "trigger",
    };
    let row = record::encode(&[
        Value::Text(kind.into()),
        Value::Text(object.name),
        Value::Text(object.table),
        Value::Integer(i64::from(object.root_page)),
        Value::Text(sql.into()),
    ]);
    tree::table_insert(pager, 1, rowid, &row, true)?;
    bump_cookie(pager)
}
