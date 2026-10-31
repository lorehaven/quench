//! `CREATE`, `DROP` and `ALTER`: changing the schema table and the trees it names.

use super::catalog::{Origin, Table, index_from_create, table_from_create};
use super::exec::Exec;
use crate::error::{Error, Result};
use crate::schema;
use crate::sql::ast::{AlterAction, ColumnConstraint, CreateIndex, CreateTable};

impl<'a> Exec<'a> {
    pub fn create_table(&mut self, ct: &CreateTable, text: &str) -> Result<()> {
        if self.catalog.name_taken(&ct.name) {
            if ct.if_not_exists {
                return Ok(());
            }
            return Err(Error::Constraint(format!(
                "table {} already exists",
                ct.name
            )));
        }
        let table = table_from_create(ct, 0)?;
        let pager = self.db.pager_mut();
        schema::create(
            pager,
            crate::ObjectKind::Table,
            &ct.name,
            &ct.name,
            Some(&strip_if_not_exists(text)),
        )?;
        for index in &table.indexes {
            schema::create(pager, crate::ObjectKind::Index, &index.name, &ct.name, None)?;
        }
        Ok(())
    }

    pub fn create_index(&mut self, ci: &CreateIndex, text: &str) -> Result<()> {
        if self.catalog.name_taken(&ci.name) {
            if ci.if_not_exists {
                return Ok(());
            }
            return Err(Error::Constraint(format!(
                "index {} already exists",
                ci.name
            )));
        }
        let catalog = self.catalog;
        let table: &'a Table = catalog.table(&ci.table)?;
        let mut index = index_from_create(table, ci)?;
        index.root = schema::create(
            self.db.pager_mut(),
            crate::ObjectKind::Index,
            &ci.name,
            &table.name,
            Some(&strip_if_not_exists(text)),
        )?;
        self.fill_index(table, &index)
    }

    pub fn drop_table(&mut self, name: &str, if_exists: bool) -> Result<()> {
        let catalog = self.catalog;
        let table = match catalog.table(name) {
            Ok(t) => t,
            Err(_) if if_exists => return Ok(()),
            Err(e) => return Err(e),
        };
        let pager = self.db.pager_mut();
        for index in &table.indexes {
            schema::drop(pager, &index.name)?;
        }
        schema::drop(pager, &table.name)?;
        Ok(())
    }

    pub fn drop_index(&mut self, name: &str, if_exists: bool) -> Result<()> {
        let found = self
            .catalog
            .tables
            .iter()
            .flat_map(|t| t.indexes.iter())
            .find(|i| i.name.eq_ignore_ascii_case(name));
        match found {
            None if if_exists => Ok(()),
            None => Err(Error::Syntax {
                pos: 0,
                message: format!("no such index: {name}"),
            }),
            Some(i) if i.origin != Origin::Created => Err(Error::Constraint(
                "index associated with UNIQUE or PRIMARY KEY constraint cannot be dropped".into(),
            )),
            Some(i) => {
                schema::drop(self.db.pager_mut(), &i.name)?;
                Ok(())
            }
        }
    }

    /// `ALTER TABLE ... ADD COLUMN`; the other forms are not done yet.
    pub fn alter(&mut self, table: &str, action: &AlterAction, text: &str) -> Result<()> {
        let AlterAction::AddColumn(def) = action else {
            return Err(Error::Unsupported(
                "ALTER TABLE other than ADD COLUMN".into(),
            ));
        };
        let t = self.catalog.table(table)?;
        if t.column(&def.name).is_some() {
            return Err(Error::Constraint(format!(
                "duplicate column name: {}",
                def.name
            )));
        }
        let has_default = def
            .constraints
            .iter()
            .any(|c| matches!(c, ColumnConstraint::Default(_)));
        for c in &def.constraints {
            match c {
                ColumnConstraint::PrimaryKey { .. } | ColumnConstraint::Unique => {
                    return Err(Error::Constraint(
                        "cannot add a PRIMARY KEY or UNIQUE column".into(),
                    ));
                }
                ColumnConstraint::NotNull if !has_default => {
                    return Err(Error::Constraint(
                        "cannot add a NOT NULL column with default value NULL".into(),
                    ));
                }
                _ => {}
            }
        }
        let lower = text.to_ascii_lowercase();
        let add = lower
            .find("add")
            .ok_or_else(|| Error::Unsupported("ALTER TABLE text".into()))?;
        let mut def_text = text[add + 3..].trim_start();
        if def_text.to_ascii_lowercase().starts_with("column") {
            def_text = def_text[6..].trim_start();
        }
        let objects = crate::schema::read(self.db.pager_mut())?;
        let old = objects
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(table) && o.sql.is_some())
            .and_then(|o| o.sql.clone())
            .ok_or_else(|| Error::corrupt(1, "the table to alter has no SQL"))?;
        let close = old
            .rfind(')')
            .ok_or_else(|| Error::corrupt(1, "the table SQL has no closing parenthesis"))?;
        let new_sql = format!("{}, {def_text}{}", &old[..close], &old[close..]);
        schema::set_sql(self.db.pager_mut(), &t.name, &new_sql)
    }
}

/// `CREATE TABLE IF NOT EXISTS x` is stored as `CREATE TABLE x`.
fn strip_if_not_exists(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    lower.find("if not exists").map_or_else(
        || text.to_string(),
        |at| {
            format!(
                "{}{}",
                &text[..at],
                text[at + "if not exists".len()..].trim_start()
            )
        },
    )
}
