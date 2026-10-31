//! The SQLite backend: a database file read and written by `quench-sqlite`.
//!
//! The engine is synchronous and one connection serves every clone of a [`SqliteDb`]; each call takes the
//! connection for the length of one statement (or one migration), so a long-running caller should not hold it
//! across `await` points - it cannot, as the calls here finish before they return.

use crate::{Crud, Database, DbError, Migration, Model};
use quench_core::async_trait;
use quench_serde::{Map, Value as Json};
use quench_sqlite::{Connection, Value as Sql};
use std::sync::{Arc, Mutex, MutexGuard};

impl From<quench_sqlite::Error> for DbError {
    fn from(err: quench_sqlite::Error) -> Self {
        match err {
            quench_sqlite::Error::Io(_)
            | quench_sqlite::Error::NotADatabase(_)
            | quench_sqlite::Error::Busy => Self::ConnectionError(err.to_string()),
            _ => Self::QueryError(err.to_string()),
        }
    }
}

/// A SQLite database file.
#[derive(Clone)]
pub struct SqliteDb {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for SqliteDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteDb").finish_non_exhaustive()
    }
}

impl SqliteDb {
    /// Opens the database file, making it if there is none.
    pub fn open(path: &str) -> Result<Self, DbError> {
        let path = path
            .strip_prefix("sqlite://")
            .or_else(|| path.strip_prefix("sqlite:"))
            .unwrap_or(path);
        Ok(Self {
            conn: Arc::new(Mutex::new(Connection::open_or_create(path)?)),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while the connection was held leaves it usable: statements are atomic.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn migration_table() -> String {
        std::env::var("DB_MIGRATION_TABLE").unwrap_or_else(|_| "quench_migrations".to_string())
    }
}

#[async_trait]
impl Database for SqliteDb {
    async fn execute(&self, query: &str) -> Result<(), DbError> {
        self.conn().execute(query, &[])?;
        Ok(())
    }

    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError> {
        let table = Self::migration_table();
        let mut conn = self.conn();
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY, author TEXT NOT NULL)"
            ),
            &[],
        )?;
        for migration in migrations {
            let seen = conn.query(
                &format!("SELECT 1 FROM {table} WHERE id = ?"),
                &[Sql::Text(migration.id.clone())],
            )?;
            if !seen.rows.is_empty() {
                continue;
            }
            quench_log::info!("Applying migration: {}", migration.id);
            conn.transaction(|c| {
                for change in &migration.changes {
                    // SQLite has no schemas or extensions: these changes have nothing to do.
                    if matches!(
                        change,
                        crate::ChangeSet::CreateSchema { .. }
                            | crate::ChangeSet::CreateExtension { .. }
                    ) {
                        continue;
                    }
                    c.execute(&change.to_sql(), &[])?;
                }
                c.execute(
                    &format!("INSERT INTO {table} (id, author) VALUES (?, ?)"),
                    &[
                        Sql::Text(migration.id.clone()),
                        Sql::Text(migration.author.clone()),
                    ],
                )?;
                Ok(())
            })?;
        }
        drop(conn);
        Ok(())
    }
}

/// A model's field as a SQL value; arrays and objects are stored as their JSON text.
// A number above the largest integer is stored as a real, as SQLite itself would.
#[allow(clippy::cast_precision_loss)]
fn to_sql(value: &Json) -> Sql {
    match value {
        Json::Null => Sql::Null,
        Json::Bool(b) => Sql::Integer(i64::from(*b)),
        Json::Int(i) => Sql::Integer(*i),
        Json::UInt(u) => i64::try_from(*u).map_or(Sql::Real(*u as f64), Sql::Integer),
        Json::Float(f) => Sql::Real(*f),
        Json::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(quench_serde::json::to_string(other)),
    }
}

/// A column as a model field: `BOOLEAN` columns hold booleans and `JSON` columns hold parsed JSON.
fn from_sql(value: Sql, declared: &str) -> Json {
    let kind = declared.to_ascii_uppercase();
    match value {
        Sql::Null => Json::Null,
        Sql::Integer(i) if kind.contains("BOOL") => Json::Bool(i != 0),
        Sql::Integer(i) => Json::from_i64(i),
        Sql::Real(r) => Json::Float(r),
        Sql::Text(s) if kind.contains("JSON") => {
            quench_serde::json::from_str::<Json>(&s).unwrap_or(Json::String(s))
        }
        Sql::Text(s) => Json::String(s),
        Sql::Blob(b) => Json::Array(
            b.into_iter()
                .map(|x| Json::from_i64(i64::from(x)))
                .collect(),
        ),
    }
}

fn key_text(json: &Json, pk: &str) -> Result<String, DbError> {
    match json.get(pk) {
        Some(Json::String(s)) => Ok(s.clone()),
        Some(Json::Int(i)) => Ok(i.to_string()),
        Some(Json::UInt(u)) => Ok(u.to_string()),
        Some(_) => Err(DbError::Unknown(format!(
            "Primary key '{pk}' is not a string or number"
        ))),
        None => Err(DbError::Unknown(format!(
            "Primary key '{pk}' not found in model"
        ))),
    }
}

/// A column and the value to store in it.
type Field = (&'static str, Sql);

pub struct SqliteRepository<T: Model> {
    db: SqliteDb,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Model> std::fmt::Debug for SqliteRepository<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteRepository").finish_non_exhaustive()
    }
}

impl<T: Model> SqliteRepository<T> {
    #[must_use]
    pub const fn new(db: SqliteDb) -> Self {
        Self {
            db,
            _marker: std::marker::PhantomData,
        }
    }

    /// Runs a query and turns each row into a model, using the declared column types to read booleans and JSON.
    fn models(&self, sql: &str, params: &[Sql]) -> Result<Vec<T>, DbError> {
        let mut conn = self.db.conn();
        let rows = conn.query(sql, params)?;
        let table = T::table_name();
        let info = conn.query(&format!("PRAGMA table_info(\"{table}\")"), &[])?;
        let types: Vec<(String, String)> = info
            .rows
            .iter()
            .filter_map(|r| {
                Some((
                    r.get(1)?.as_text()?.to_string(),
                    r.get(2)?.as_text().unwrap_or("").to_string(),
                ))
            })
            .collect();
        drop(conn);
        rows.rows
            .into_iter()
            .map(|row| {
                let mut object = Map::new();
                for (name, value) in rows.columns.iter().zip(row) {
                    let declared = types
                        .iter()
                        .find(|(n, _)| n == name)
                        .map_or("", |(_, t)| t.as_str());
                    object.insert(name.clone(), from_sql(value, declared));
                }
                quench_serde::from_value::<T>(&Json::Object(object)).map_err(DbError::from)
            })
            .collect()
    }

    /// The model as JSON, and its fields that the table has as `(column, value)`.
    fn fields(model: &T) -> (Json, Vec<Field>) {
        let json = quench_serde::to_value(model);
        let fields = T::columns()
            .into_iter()
            .filter_map(|c| json.get(c).map(|v| (c, to_sql(v))))
            .collect();
        (json, fields)
    }
}

#[async_trait]
impl<T> Crud<T> for SqliteRepository<T>
where
    T: Model,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        let (_, fields) = Self::fields(model);
        let table = T::table_name();
        let names = fields
            .iter()
            .map(|(c, _)| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let marks = vec!["?"; fields.len()].join(", ");
        let params: Vec<Sql> = fields.into_iter().map(|(_, v)| v).collect();
        let sql = if params.is_empty() {
            format!("INSERT INTO \"{table}\" DEFAULT VALUES RETURNING *")
        } else {
            format!("INSERT INTO \"{table}\" ({names}) VALUES ({marks}) RETURNING *")
        };
        self.models(&sql, &params)?
            .into_iter()
            .next()
            .ok_or(DbError::NotFound)
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        let sql = format!(
            "SELECT * FROM \"{}\" WHERE \"{}\" = ?",
            T::table_name(),
            T::primary_key_name()
        );
        Ok(self
            .models(&sql, &[Sql::Text(id.to_string())])?
            .into_iter()
            .next())
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        let (json, fields) = Self::fields(model);
        let pk = T::primary_key_name();
        let key = key_text(&json, &pk)?;
        let sets = fields
            .iter()
            .map(|(c, _)| format!("\"{c}\" = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut params: Vec<Sql> = fields.into_iter().map(|(_, v)| v).collect();
        params.push(Sql::Text(key));
        let sql = format!(
            "UPDATE \"{}\" SET {sets} WHERE \"{pk}\" = ? RETURNING *",
            T::table_name()
        );
        self.models(&sql, &params)?
            .into_iter()
            .next()
            .ok_or(DbError::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        let sql = format!(
            "DELETE FROM \"{}\" WHERE \"{}\" = ?",
            T::table_name(),
            T::primary_key_name()
        );
        self.db.conn().execute(&sql, &[Sql::Text(id.to_string())])?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        self.models(&format!("SELECT * FROM \"{}\"", T::table_name()), &[])
    }

    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError> {
        let column = crate::checked_column::<T>(column)?;
        let sql = format!(
            "SELECT * FROM \"{}\" WHERE \"{column}\" = ?",
            T::table_name()
        );
        self.models(&sql, &[Sql::Text(value.to_string())])
    }
}
