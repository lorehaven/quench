use crate::{Crud, Database, DbError, Migration, Model};
use quench_core::async_trait;
use quench_pg::{Config, Pool, PoolOptions, Row};
use quench_serde::Value;
use std::fmt::Debug;

#[derive(Clone)]
pub struct PostgresDb {
    pool: Pool,
}

impl PostgresDb {
    pub async fn new(url: &str) -> Result<Self, DbError> {
        let max_size = std::env::var("DB_POOL_MAX_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .unwrap_or(5);
        let pool = Pool::connect_with(
            Config::from_url(url)?,
            PoolOptions {
                max_size,
                ..PoolOptions::default()
            },
        )
        .await?;
        Ok(Self { pool })
    }

    #[must_use]
    pub const fn pool(&self) -> &Pool {
        &self.pool
    }

    fn migration_table() -> String {
        std::env::var("DB_MIGRATION_TABLE").unwrap_or_else(|_| "quench_migrations".to_string())
    }

    async fn ensure_migration_table(&self) -> Result<(), DbError> {
        let table = Self::migration_table();
        let query = format!(
            "
            CREATE TABLE IF NOT EXISTS {table} (
                id TEXT PRIMARY KEY,
                author TEXT NOT NULL,
                applied_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
            )
        "
        );
        self.execute(&query).await
    }
}

impl Debug for PostgresDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresDb").finish()
    }
}

#[async_trait]
impl Database for PostgresDb {
    async fn execute(&self, query: &str) -> Result<(), DbError> {
        // The simple protocol, so a statement list (a migration script) is fine.
        self.pool.batch_execute(query).await?;
        Ok(())
    }

    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError> {
        self.ensure_migration_table().await?;
        let table = Self::migration_table();

        for migration in migrations {
            let query = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id = $1)");
            let exists = self
                .pool
                .query_one(&query, &[&migration.id])
                .await?
                .get::<bool, _>(&0usize)?;

            if !exists {
                quench_log::info!("Applying migration: {}", migration.id);
                let mut conn = self.pool.get().await?;
                let mut tx = conn.begin().await?;

                for change in migration.changes {
                    tx.batch_execute(&change.to_sql()).await?;
                }

                let insert_query = format!("INSERT INTO {table} (id, author) VALUES ($1, $2)");
                tx.execute(&insert_query, &[&migration.id, &migration.author])
                    .await?;

                tx.commit().await?;
                drop(conn);
            }
        }

        Ok(())
    }
}

impl<T: Model> std::fmt::Debug for PostgresRepository<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresRepository").finish_non_exhaustive()
    }
}

pub struct PostgresRepository<T: Model> {
    db: PostgresDb,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Model> PostgresRepository<T> {
    #[must_use]
    pub const fn new(db: PostgresDb) -> Self {
        Self {
            db,
            _marker: std::marker::PhantomData,
        }
    }
}

fn model_from_row<T: Model>(row: &Row) -> Result<T, DbError> {
    Ok(row.deserialize::<T>()?)
}

#[async_trait]
impl<T> Crud<T> for PostgresRepository<T>
where
    T: Model,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        let table = T::table_name();
        let json = quench_serde::to_value(model);
        let query = format!(
            "INSERT INTO {table} SELECT * FROM jsonb_populate_record(NULL::{table}, $1) RETURNING *"
        );

        let row = self.db.pool.query_one(&query, &[&json]).await?;
        model_from_row(&row)
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        let query = format!(
            "SELECT * FROM {} WHERE {} = $1",
            T::table_name(),
            T::primary_key_name()
        );
        self.db
            .pool
            .query_opt(&query, &[&id])
            .await?
            .as_ref()
            .map(model_from_row)
            .transpose()
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        let table = T::table_name();
        let pk = T::primary_key_name();
        let columns = T::columns();
        let json = quench_serde::to_value(model);

        let cols_list = columns.join(", ");

        let query = format!(
            "UPDATE {table} SET ({cols_list}) = (SELECT {cols_list} FROM jsonb_populate_record(NULL::{table}, $1)) WHERE {pk} = $2 RETURNING *"
        );

        let id_val = json
            .get(pk.as_str())
            .ok_or_else(|| DbError::Unknown(format!("Primary key '{pk}' not found in model")))?;

        let id = match id_val {
            Value::String(s) => s.clone(),
            Value::Int(n) => n.to_string(),
            Value::UInt(n) => n.to_string(),
            _ => {
                return Err(DbError::Unknown(format!(
                    "Primary key '{pk}' is not a string or number"
                )));
            }
        };

        let row = self.db.pool.query_one(&query, &[&json, &id]).await?;
        model_from_row(&row)
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        let query = format!(
            "DELETE FROM {} WHERE {} = $1",
            T::table_name(),
            T::primary_key_name()
        );
        self.db.pool.execute(&query, &[&id]).await?;
        Ok(())
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        let query = format!("SELECT * FROM {}", T::table_name());
        self.db
            .pool
            .query(&query, &[])
            .await?
            .iter()
            .map(model_from_row)
            .collect()
    }

    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError> {
        let column = crate::checked_column::<T>(column)?;
        let query = format!("SELECT * FROM {} WHERE {} = $1", T::table_name(), column);
        self.db
            .pool
            .query(&query, &[&value])
            .await?
            .iter()
            .map(model_from_row)
            .collect()
    }
}
