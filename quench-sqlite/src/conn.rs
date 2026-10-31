//! A connection: SQL in, rows and changes out, with transactions and statement-level atomicity.

use std::path::Path;

use crate::Database;
use crate::engine::catalog::Catalog;
use crate::engine::eval::Params;
use crate::engine::exec::{Exec, Rows};
use crate::error::{Error, Result};
use crate::pager::Snapshot;
use crate::record::Value;
use crate::sql::ast::Stmt;
use crate::sql::parse_with_text;

/// A database opened for SQL.
#[derive(Debug)]
pub struct Connection {
    db: Database,
    catalog: Option<Catalog>,
    /// Inside a `BEGIN`.
    explicit: bool,
    savepoints: Vec<(String, Snapshot)>,
    last_insert_rowid: i64,
    changes: u64,
}

fn unsupported_here(what: &str) -> Error {
    Error::Unsupported(what.into())
}

impl Connection {
    /// Opens an existing database for reading and writing.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::wrap(Database::open_rw(path)?))
    }

    /// Makes a new database file (4096-byte pages) and opens it.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::wrap(Database::create(path, 4096)?))
    }

    /// Opens the file, making the database first if there is none.
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self> {
        if path.as_ref().exists() {
            Self::open(path)
        } else {
            Self::create(path)
        }
    }

    const fn wrap(db: Database) -> Self {
        Self {
            db,
            catalog: None,
            explicit: false,
            savepoints: Vec::new(),
            last_insert_rowid: 0,
            changes: 0,
        }
    }

    /// The rowid of the last row inserted.
    #[must_use]
    pub const fn last_insert_rowid(&self) -> i64 {
        self.last_insert_rowid
    }

    /// The database under the connection (tests).
    #[cfg(test)]
    pub(crate) const fn database(&mut self) -> &mut Database {
        &mut self.db
    }

    /// How many rows the last statement changed.
    #[must_use]
    pub const fn changes(&self) -> u64 {
        self.changes
    }

    /// Whether a `BEGIN` is open.
    #[must_use]
    pub const fn in_transaction(&self) -> bool {
        self.explicit
    }

    /// Runs the statements in `sql` with positional parameters (`?`, `?N`). Returns the rows changed by the last.
    pub fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64> {
        self.run(
            sql,
            Params {
                positional: params,
                named: &[],
            },
        )
        .map(|(_, changes)| changes)
    }

    /// Like [`execute`](Self::execute), with named parameters (`:name`, `@name`, `$name`) as well.
    pub fn execute_named(
        &mut self,
        sql: &str,
        positional: &[Value],
        named: &[(&str, Value)],
    ) -> Result<u64> {
        self.run(sql, Params { positional, named })
            .map(|(_, changes)| changes)
    }

    /// Runs the statements and returns the rows of the last (a `SELECT`, or anything with `RETURNING`).
    pub fn query(&mut self, sql: &str, params: &[Value]) -> Result<Rows> {
        self.run(
            sql,
            Params {
                positional: params,
                named: &[],
            },
        )
        .map(|(rows, _)| rows)
    }

    /// Like [`query`](Self::query), with named parameters as well.
    pub fn query_named(
        &mut self,
        sql: &str,
        positional: &[Value],
        named: &[(&str, Value)],
    ) -> Result<Rows> {
        self.run(sql, Params { positional, named })
            .map(|(rows, _)| rows)
    }

    /// Runs `f` in a transaction: committed if it returns `Ok`, rolled back if it returns an error.
    pub fn transaction<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.execute("BEGIN", &[])?;
        match f(self) {
            Ok(v) => {
                self.execute("COMMIT", &[])?;
                Ok(v)
            }
            Err(e) => {
                let _ = self.execute("ROLLBACK", &[]);
                Err(e)
            }
        }
    }

    fn run(&mut self, sql: &str, params: Params) -> Result<(Rows, u64)> {
        let mut last = (Rows::default(), 0);
        for (stmt, text) in parse_with_text(sql)? {
            last = self.run_stmt(&stmt, &text, params)?;
        }
        Ok(last)
    }

    fn ensure_catalog(&mut self) -> Result<()> {
        if self.catalog.is_none() {
            self.catalog = Some(Catalog::build(&self.db.objects()?));
        }
        Ok(())
    }

    fn run_stmt(&mut self, stmt: &Stmt, text: &str, params: Params) -> Result<(Rows, u64)> {
        let none = (Rows::default(), 0);
        match stmt {
            Stmt::Begin(_) => {
                if self.explicit {
                    return Err(Error::Constraint(
                        "cannot start a transaction within a transaction".into(),
                    ));
                }
                self.db.begin()?;
                self.explicit = true;
                Ok(none)
            }
            Stmt::Commit => {
                if !self.explicit {
                    return Err(Error::Constraint(
                        "cannot commit - no transaction is active".into(),
                    ));
                }
                self.db.commit()?;
                self.explicit = false;
                self.savepoints.clear();
                Ok(none)
            }
            Stmt::Rollback => {
                if !self.explicit {
                    return Err(Error::Constraint(
                        "cannot rollback - no transaction is active".into(),
                    ));
                }
                self.db.rollback();
                self.explicit = false;
                self.savepoints.clear();
                self.catalog = None;
                Ok(none)
            }
            Stmt::Savepoint(name) => {
                if !self.explicit {
                    return Err(unsupported_here("SAVEPOINT outside a BEGIN"));
                }
                let snap = self.db.snapshot()?;
                self.savepoints.push((name.clone(), snap));
                Ok(none)
            }
            Stmt::Release(name) => {
                let at = self.savepoint(name)?;
                self.savepoints.truncate(at);
                Ok(none)
            }
            Stmt::RollbackTo(name) => {
                let at = self.savepoint(name)?;
                self.savepoints.truncate(at + 1);
                let snap = self.savepoints[at].1.clone();
                self.db.restore(snap)?;
                self.catalog = None;
                Ok(none)
            }
            Stmt::Pragma { name, value } => self.pragma(name, value.as_ref()),
            _ => self.run_data(stmt, text, params),
        }
    }

    fn savepoint(&self, name: &str) -> Result<usize> {
        self.savepoints
            .iter()
            .rposition(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::Constraint(format!("no such savepoint: {name}")))
    }

    fn run_data(&mut self, stmt: &Stmt, text: &str, params: Params) -> Result<(Rows, u64)> {
        let writes = !matches!(stmt, Stmt::Select(_));
        let ddl = matches!(
            stmt,
            Stmt::CreateTable(_)
                | Stmt::CreateIndex(_)
                | Stmt::DropTable { .. }
                | Stmt::DropIndex { .. }
                | Stmt::AlterTable { .. }
        );
        let auto = !self.explicit && writes;
        if auto {
            self.db.begin()?;
        }
        // Inside a BEGIN, a statement that fails is undone alone: the pager logs the pages it touches.
        let tracked = self.explicit && writes;
        if tracked {
            self.db.begin_statement()?;
        }
        let result = self.exec_stmt(stmt, text, params);
        if result.is_ok() {
            if tracked {
                self.db.end_statement();
            }
            if auto {
                self.db.commit()?;
            }
        } else if auto {
            self.db.rollback();
        } else if tracked {
            self.db.abort_statement();
        }
        if ddl {
            self.catalog = None;
        }
        result
    }

    fn exec_stmt(&mut self, stmt: &Stmt, text: &str, params: Params) -> Result<(Rows, u64)> {
        self.ensure_catalog()?;
        let catalog = self
            .catalog
            .as_ref()
            .ok_or_else(|| Error::corrupt(0, "internal: no catalog"))?;
        let mut ex = Exec {
            db: &mut self.db,
            catalog,
            params,
            last_rowid: self.last_insert_rowid,
            changes: 0,
        };
        let rows = match stmt {
            Stmt::Select(s) => ex.select(s, &[])?,
            Stmt::Insert(i) => ex.insert(i, &[])?,
            Stmt::Update(u) => ex.update(u, &[])?,
            Stmt::Delete(d) => ex.delete(d, &[])?,
            Stmt::CreateTable(ct) => {
                ex.create_table(ct, text)?;
                Rows::default()
            }
            Stmt::CreateIndex(ci) => {
                ex.create_index(ci, text)?;
                Rows::default()
            }
            Stmt::DropTable { name, if_exists } => {
                ex.drop_table(name, *if_exists)?;
                Rows::default()
            }
            Stmt::DropIndex { name, if_exists } => {
                ex.drop_index(name, *if_exists)?;
                Rows::default()
            }
            Stmt::AlterTable { table, action } => {
                ex.alter(table, action, text)?;
                Rows::default()
            }
            _ => return Err(unsupported_here("this statement")),
        };
        let (changes, rowid) = (ex.changes, ex.last_rowid);
        self.last_insert_rowid = rowid;
        self.changes = changes;
        Ok((rows, changes))
    }

    fn pragma(&mut self, name: &str, value: Option<&Value>) -> Result<(Rows, u64)> {
        let one = |v: Value| Rows {
            columns: vec![name.to_string()],
            rows: vec![vec![v]],
        };
        let int = |n: u32| Value::Integer(i64::from(n));
        let lower = name.to_ascii_lowercase();
        match (lower.as_str(), value) {
            ("user_version", None) => Ok((one(int(self.db.header().user_version)), 0)),
            ("user_version", Some(v)) => {
                let n = match v {
                    Value::Integer(n) => *n as u32,
                    _ => return Err(unsupported_here("user_version must be an integer")),
                };
                let auto = !self.explicit;
                if auto {
                    self.db.begin()?;
                }
                let r = self.db.set_user_version(n);
                if auto {
                    if r.is_ok() {
                        self.db.commit()?;
                    } else {
                        self.db.rollback();
                    }
                }
                r.map(|()| (Rows::default(), 0))
            }
            ("schema_version", None) => Ok((one(int(self.db.header().schema_cookie)), 0)),
            ("page_count", None) => Ok((one(int(self.db.page_count())), 0)),
            ("page_size", None) => Ok((one(int(self.db.header().page_size)), 0)),
            ("freelist_count", None) => Ok((one(int(self.db.header().freelist_count)), 0)),
            ("encoding", None) => Ok((one(Value::Text("UTF-8".into())), 0)),
            ("journal_mode", None) => Ok((one(Value::Text("delete".into())), 0)),
            ("journal_mode", Some(Value::Text(m))) if m.eq_ignore_ascii_case("delete") => {
                Ok((one(Value::Text("delete".into())), 0))
            }
            ("integrity_check" | "quick_check", _) => {
                let problems = crate::engine::check::integrity(self.db.pager_mut())?;
                let rows = if problems.is_empty() {
                    vec![vec![Value::Text("ok".into())]]
                } else {
                    problems.into_iter().map(|p| vec![Value::Text(p)]).collect()
                };
                Ok((
                    Rows {
                        columns: vec!["integrity_check".into()],
                        rows,
                    },
                    0,
                ))
            }
            ("foreign_keys", None) => Ok((one(Value::Integer(0)), 0)),
            ("foreign_keys", Some(Value::Integer(0)))
            | ("synchronous" | "cache_size" | "busy_timeout" | "temp_store" | "locking_mode", _) => {
                Ok((Rows::default(), 0))
            }
            ("table_info", Some(Value::Text(t))) => {
                self.ensure_catalog()?;
                let table = self
                    .catalog
                    .as_ref()
                    .ok_or_else(|| Error::corrupt(0, "internal: no catalog"))?
                    .table(t)?;
                let pk: Vec<usize> = table.rowid_alias.into_iter().collect();
                let rows = table
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        vec![
                            Value::Integer(i as i64),
                            Value::Text(c.name.clone()),
                            Value::Text(c.type_name.clone().unwrap_or_default()),
                            Value::Integer(i64::from(c.not_null)),
                            c.default
                                .as_ref()
                                .and_then(crate::engine::catalog::constant)
                                .unwrap_or(Value::Null),
                            Value::Integer(i64::from(pk.contains(&i))),
                        ]
                    })
                    .collect();
                Ok((
                    Rows {
                        columns: ["cid", "name", "type", "notnull", "dflt_value", "pk"]
                            .map(String::from)
                            .to_vec(),
                        rows,
                    },
                    0,
                ))
            }
            _ => Err(Error::Unsupported(format!("PRAGMA {name}"))),
        }
    }
}
