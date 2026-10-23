//! A PostgreSQL client: the v3 wire protocol over TCP, Unix sockets or TLS,
//! SCRAM-SHA-256 authentication, parameterised queries with typed rows,
//! transactions and a connection pool.
//!
//! ```no_run
//! # async fn demo() -> Result<(), quench_pg::Error> {
//! let pool = quench_pg::Pool::connect("postgres://app:secret@localhost/app").await?;
//! let rows = pool.query("SELECT id, name FROM users WHERE name = $1", &[&"ann"]).await?;
//! for row in rows {
//!     let (id, name): (i32, String) = (row.get("id")?, row.get("name")?);
//! }
//! # Ok(()) }
//! ```
//!
//! Everything travels in the text format. Parameters implement [`ToSql`]; a
//! column is read as a Rust type through [`FromSql`] (`row.get`), or the whole
//! row is read into any `quench_serde::Deserialize` type (`row.deserialize`),
//! which is how `quench-db` maps models.

mod config;
mod conn;
mod error;
pub mod message;
mod pool;
mod row;
pub mod scram;
pub mod types;

pub use config::{Config, SslMode};
pub use conn::{Connection, QueryResult, Stream, Transaction};
pub use error::{Error, Result, ServerError};
pub use pool::{Pool, PoolOptions, PooledConnection};
pub use row::{Row, RowIndex};
pub use types::{Column, FromSql, PgArray, ToSql};
