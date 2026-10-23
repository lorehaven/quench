use quench_core::async_trait;
use quench_serde::{DeserializeOwned, Serialize};
use std::fmt::Debug;

pub mod backends;
pub mod catalog;
pub mod error;
pub mod migrations;
pub mod plan;
pub mod prelude;
#[cfg(feature = "postgres")]
pub mod runner;

pub use backends::in_memory::{InMemoryDb, InMemoryRepository};
#[cfg(feature = "postgres")]
pub use backends::postgres::{PostgresDb, PostgresRepository};
pub use catalog::{Catalog, CatalogError, CatalogModule, ModuleManifest, ModuleScope, Requirement};
pub use error::DbError;
pub use migrations::{ChangeSet, ColumnDef, Migration, MigrationFile, MigrationLoader};
pub use plan::{InstallRequest, MigrationPlan, PlanError, PlannedMigration, ResolvedModule};
#[cfg(feature = "postgres")]
pub use runner::{ApplyReport, MigrationOutcome, MigrationResult, MigrationRunner, ResetReport};

#[async_trait]
pub trait Database: Send + Sync + Debug {
    async fn execute(&self, query: &str) -> Result<(), DbError>;
    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError>;
}

#[async_trait]
pub trait Model: Serialize + DeserializeOwned + Send + Sync + Debug + Unpin + Clone {
    fn table_name() -> String;
    fn columns() -> Vec<&'static str>;
    #[must_use]
    fn primary_key_name() -> String {
        "id".to_string()
    }
}

#[async_trait]
pub trait Crud<T: Model>: Send + Sync {
    async fn create(&self, model: &T) -> Result<T, DbError>;
    async fn read(&self, id: &str) -> Result<Option<T>, DbError>;
    async fn update(&self, model: &T) -> Result<T, DbError>;
    async fn delete(&self, id: &str) -> Result<(), DbError>;
    async fn list(&self) -> Result<Vec<T>, DbError>;

    /// Rows where `column` equals `value`.
    ///
    /// The value binds as a parameter; the column cannot, so it is checked
    /// against [`Model::columns`] before being interpolated.
    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError>;
}

/// Rejects a column that the model does not declare, so a caller cannot smuggle
/// SQL through the identifier position of `find_by`.
pub(crate) fn checked_column<T: Model>(column: &str) -> Result<&'static str, DbError> {
    T::columns()
        .into_iter()
        .find(|known| *known == column)
        .ok_or_else(|| {
            DbError::Unknown(format!(
                "unknown column '{column}' for table {}",
                T::table_name()
            ))
        })
}

#[derive(Clone, Debug)]
pub enum Db {
    #[cfg(feature = "postgres")]
    Postgres(PostgresDb),
    InMemory(InMemoryDb),
}

impl Db {
    #[cfg_attr(
        not(feature = "postgres"),
        allow(clippy::unused_async, clippy::unused_async_trait_impl)
    )] // only Postgres connects asynchronously
    pub async fn connect(url: &str) -> Result<Self, DbError> {
        if url.is_empty() {
            return Ok(Self::InMemory(InMemoryDb::new()));
        }
        #[cfg(feature = "postgres")]
        return Ok(Self::Postgres(PostgresDb::new(url).await?));
        #[cfg(not(feature = "postgres"))]
        Err(DbError::ConnectionError(format!(
            "cannot connect to {url}: quench-db was built without its `postgres` feature"
        )))
    }

    #[must_use]
    pub fn repository<T: Model>(&self) -> Repository<T> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(db) => Repository::Postgres(PostgresRepository::new(db.clone())),
            Self::InMemory(db) => Repository::InMemory(InMemoryRepository::new(db.clone())),
        }
    }
}

#[async_trait]
impl Database for Db {
    async fn execute(&self, query: &str) -> Result<(), DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(db) => db.execute(query).await,
            Self::InMemory(db) => db.execute(query).await,
        }
    }

    async fn migrate(&self, migrations: Vec<Migration>) -> Result<(), DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(db) => db.migrate(migrations).await,
            Self::InMemory(db) => db.migrate(migrations).await,
        }
    }
}

impl<T: Model> std::fmt::Debug for Repository<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => f.debug_tuple("Postgres").field(repo).finish(),
            Self::InMemory(repo) => f.debug_tuple("InMemory").field(repo).finish(),
        }
    }
}

pub enum Repository<T: Model> {
    #[cfg(feature = "postgres")]
    Postgres(PostgresRepository<T>),
    InMemory(InMemoryRepository<T>),
}

#[async_trait]
impl<T> Crud<T> for Repository<T>
where
    T: Model,
{
    async fn create(&self, model: &T) -> Result<T, DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.create(model).await,
            Self::InMemory(repo) => repo.create(model).await,
        }
    }

    async fn read(&self, id: &str) -> Result<Option<T>, DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.read(id).await,
            Self::InMemory(repo) => repo.read(id).await,
        }
    }

    async fn update(&self, model: &T) -> Result<T, DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.update(model).await,
            Self::InMemory(repo) => repo.update(model).await,
        }
    }

    async fn delete(&self, id: &str) -> Result<(), DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.delete(id).await,
            Self::InMemory(repo) => repo.delete(id).await,
        }
    }

    async fn list(&self) -> Result<Vec<T>, DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.list().await,
            Self::InMemory(repo) => repo.list().await,
        }
    }

    async fn find_by(&self, column: &str, value: &str) -> Result<Vec<T>, DbError> {
        match self {
            #[cfg(feature = "postgres")]
            Self::Postgres(repo) => repo.find_by(column, value).await,
            Self::InMemory(repo) => repo.find_by(column, value).await,
        }
    }
}
