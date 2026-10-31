//! The schema as the executor needs it: tables with their columns, constraints and indexes, built from the
//! `CREATE` text stored in `sqlite_schema`.

use super::value::{Affinity, Collation, affinity_of};
use crate::error::{Error, Result};
use crate::record::Value;
use crate::schema::{Object, ObjectKind};
use crate::sql::ast::{
    ColumnConstraint, CreateTable, Expr, IndexedColumn, Stmt, TableConstraint, UnaryOp,
};
use crate::sql::parse_one;

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub type_name: Option<String>,
    pub affinity: Affinity,
    pub not_null: bool,
    pub default: Option<Expr>,
    pub collation: Collation,
}

/// Where an index came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `CREATE INDEX`.
    Created,
    /// A `UNIQUE` constraint.
    Unique,
    /// A `PRIMARY KEY` that is not the rowid.
    PrimaryKey,
}

#[derive(Debug, Clone)]
pub struct Index {
    pub name: String,
    pub columns: Vec<usize>,
    pub unique: bool,
    pub root: u32,
    pub origin: Origin,
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    /// The column that is the rowid (`INTEGER PRIMARY KEY`).
    pub rowid_alias: Option<usize>,
    pub root: u32,
    pub checks: Vec<Expr>,
    pub indexes: Vec<Index>,
}

impl Table {
    /// The column named `name`, case blind.
    #[must_use]
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The values of a stored row as the columns hold them: the rowid alias is the rowid, a column the record is
    /// too short for (added later) takes its default, a `REAL` column holds a real.
    #[must_use]
    pub fn expand_row(&self, rowid: i64, mut stored: Vec<Value>) -> Vec<Value> {
        for (i, column) in self.columns.iter().enumerate() {
            if i >= stored.len() {
                stored.push(
                    column
                        .default
                        .as_ref()
                        .and_then(constant)
                        .unwrap_or(Value::Null),
                );
            }
            if self.rowid_alias == Some(i) {
                stored[i] = Value::Integer(rowid);
            } else if column.affinity == Affinity::Real
                && let Value::Integer(n) = stored[i]
            {
                stored[i] = Value::Real(n as f64);
            }
        }
        stored.truncate(self.columns.len());
        stored
    }
}

/// The value of an expression made of literals, if it is one.
#[must_use]
pub fn constant(e: &Expr) -> Option<Value> {
    match e {
        Expr::Literal(v) => Some(v.clone()),
        Expr::Unary(UnaryOp::Neg, inner) => match constant(inner)? {
            Value::Integer(i) => Some(Value::Integer(-i)),
            Value::Real(r) => Some(Value::Real(-r)),
            _ => None,
        },
        Expr::Unary(UnaryOp::Plus, inner) => constant(inner),
        _ => None,
    }
}

/// Every table, with its indexes.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub tables: Vec<Table>,
    /// Tables the executor cannot use (SQL it does not support), with the reason.
    pub broken: Vec<(String, String)>,
}

impl Catalog {
    /// Looks a table up by name, case blind.
    pub fn table(&self, name: &str) -> Result<&Table> {
        if let Some(t) = self
            .tables
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
        {
            return Ok(t);
        }
        if let Some((_, why)) = self
            .broken
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return Err(Error::Unsupported(format!("the table {name}: {why}")));
        }
        Err(Error::Syntax {
            pos: 0,
            message: format!("no such table: {name}"),
        })
    }

    /// Whether any table or index has this name.
    #[must_use]
    pub fn name_taken(&self, name: &str) -> bool {
        self.tables.iter().any(|t| {
            t.name.eq_ignore_ascii_case(name)
                || t.indexes.iter().any(|i| i.name.eq_ignore_ascii_case(name))
        }) || self
            .broken
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Builds the catalog from the rows of `sqlite_schema`.
    pub fn build(objects: &[Object]) -> Self {
        let mut catalog = Self::default();
        for object in objects.iter().filter(|o| o.kind == ObjectKind::Table) {
            if object.name.starts_with("sqlite_") {
                continue;
            }
            match Self::load_table(object, objects) {
                Ok(table) => catalog.tables.push(table),
                Err(e) => catalog.broken.push((object.name.clone(), e.to_string())),
            }
        }
        // Explicit indexes.
        for object in objects.iter().filter(|o| o.kind == ObjectKind::Index) {
            let Some(sql) = &object.sql else { continue };
            let Some(table) = catalog
                .tables
                .iter_mut()
                .find(|t| t.name.eq_ignore_ascii_case(&object.table))
            else {
                continue;
            };
            let built = parse_one(sql)
                .map_err(|e| e.to_string())
                .and_then(|stmt| match stmt {
                    Stmt::CreateIndex(ci) => index_from_columns(
                        table,
                        &ci.name,
                        ci.unique,
                        &ci.columns,
                        Origin::Created,
                        object.root_page,
                    )
                    .map_err(|e| e.to_string()),
                    _ => Err("not an index".into()),
                });
            match built {
                Ok(index) => table.indexes.push(index),
                Err(why) => {
                    let name = table.name.clone();
                    catalog.tables.retain(|t| t.name != name);
                    catalog
                        .broken
                        .push((name, format!("index {}: {why}", object.name)));
                }
            }
        }
        catalog
    }

    fn load_table(object: &Object, objects: &[Object]) -> Result<Table> {
        let sql = object
            .sql
            .as_deref()
            .ok_or_else(|| Error::corrupt(0, "a table without SQL"))?;
        let Stmt::CreateTable(ct) = parse_one(sql)? else {
            return Err(Error::corrupt(0, "the SQL of a table is not CREATE TABLE"));
        };
        let mut table = table_from_create(&ct, object.root_page)?;
        // The automatic indexes (no SQL) follow the constraints in the order they were declared.
        let autos: Vec<&Object> = objects
            .iter()
            .filter(|o| {
                o.kind == ObjectKind::Index
                    && o.sql.is_none()
                    && o.table.eq_ignore_ascii_case(&object.name)
            })
            .collect();
        if autos.len() != table.indexes.len() {
            return Err(Error::corrupt(
                0,
                "the automatic indexes do not match the constraints",
            ));
        }
        for (index, auto) in table.indexes.iter_mut().zip(autos) {
            index.root = auto.root_page;
            index.name.clone_from(&auto.name);
        }
        Ok(table)
    }
}

fn index_from_columns(
    table: &Table,
    name: &str,
    unique: bool,
    columns: &[IndexedColumn],
    origin: Origin,
    root: u32,
) -> Result<Index> {
    let mut cols = Vec::new();
    for c in columns {
        if c.descending {
            return Err(Error::Unsupported("DESC in an index".into()));
        }
        let i = table.column(&c.name).ok_or_else(|| {
            Error::Unsupported(format!("no such column in the index: {}", c.name))
        })?;
        if table.columns[i].collation != Collation::Binary {
            return Err(Error::Unsupported(
                "an index on a column with a collation other than BINARY".into(),
            ));
        }
        cols.push(i);
    }
    Ok(Index {
        name: name.to_string(),
        columns: cols,
        unique,
        root,
        origin,
    })
}

/// Builds a table (and the automatic indexes its constraints call for, with root 0) from `CREATE TABLE`.
pub fn table_from_create(ct: &CreateTable, root: u32) -> Result<Table> {
    if ct.without_rowid {
        return Err(Error::Unsupported("WITHOUT ROWID tables".into()));
    }
    let mut columns = Vec::new();
    for def in &ct.columns {
        let mut column = Column {
            name: def.name.clone(),
            type_name: def.type_name.clone(),
            affinity: affinity_of(def.type_name.as_deref()),
            not_null: false,
            default: None,
            collation: Collation::Binary,
        };
        for c in &def.constraints {
            match c {
                ColumnConstraint::NotNull => column.not_null = true,
                ColumnConstraint::Default(e) => column.default = Some(e.clone()),
                ColumnConstraint::Collate(name) => {
                    column.collation = Collation::by_name(name)
                        .ok_or_else(|| Error::Unsupported(format!("the collation {name}")))?;
                }
                ColumnConstraint::PrimaryKey {
                    autoincrement: true,
                    ..
                } => {
                    return Err(Error::Unsupported("AUTOINCREMENT".into()));
                }
                _ => {}
            }
        }
        if columns
            .iter()
            .any(|c: &Column| c.name.eq_ignore_ascii_case(&column.name))
        {
            return Err(Error::Unsupported(format!(
                "duplicate column name: {}",
                column.name
            )));
        }
        columns.push(column);
    }
    let mut table = Table {
        name: ct.name.clone(),
        columns,
        rowid_alias: None,
        root,
        checks: Vec::new(),
        indexes: Vec::new(),
    };
    // Constraints in declaration order: column constraints first, then the table's.
    let mut keys: Vec<(Vec<IndexedColumn>, Origin)> = Vec::new();
    let mut primary: Option<Vec<IndexedColumn>> = None;
    for def in &ct.columns {
        for c in &def.constraints {
            match c {
                ColumnConstraint::PrimaryKey { descending, .. } => {
                    primary = Some(vec![IndexedColumn {
                        name: def.name.clone(),
                        descending: *descending,
                    }]);
                }
                ColumnConstraint::Unique => keys.push((
                    vec![IndexedColumn {
                        name: def.name.clone(),
                        descending: false,
                    }],
                    Origin::Unique,
                )),
                ColumnConstraint::Check(e) => table.checks.push(e.clone()),
                _ => {}
            }
        }
    }
    for c in &ct.constraints {
        match c {
            TableConstraint::PrimaryKey(cols) => primary = Some(cols.clone()),
            TableConstraint::Unique(cols) => keys.push((cols.clone(), Origin::Unique)),
            TableConstraint::Check(e) => table.checks.push(e.clone()),
        }
    }
    if let Some(cols) = &primary {
        let alias = cols.len() == 1
            && !cols[0].descending
            && table.column(&cols[0].name).is_some_and(|i| {
                table.columns[i]
                    .type_name
                    .as_deref()
                    .is_some_and(|t| t.eq_ignore_ascii_case("INTEGER"))
            });
        if alias {
            table.rowid_alias = table.column(&cols[0].name);
        } else {
            keys.insert(0, (cols.clone(), Origin::PrimaryKey));
        }
    }
    let mut seen: Vec<Vec<usize>> = Vec::new();
    for (cols, origin) in keys {
        let mut index = index_from_columns(&table, "", true, &cols, origin, 0)?;
        // The primary key's columns are never null in practice here; SQLite lets them be, and so do we.
        if seen.contains(&index.columns) {
            continue;
        }
        seen.push(index.columns.clone());
        index.name = format!("sqlite_autoindex_{}_{}", table.name, seen.len());
        table.indexes.push(index);
    }
    Ok(table)
}

/// Builds the index of a `CREATE INDEX` on `table` (root 0 until it is created).
pub fn index_from_create(table: &Table, ci: &crate::sql::ast::CreateIndex) -> Result<Index> {
    index_from_columns(table, &ci.name, ci.unique, &ci.columns, Origin::Created, 0)
}
