use quench_core::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database connection error: {0}")]
    ConnectionError(String),

    #[error("Query execution error: {0}")]
    QueryError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Record not found")]
    NotFound,

    #[error("Unknown database error: {0}")]
    Unknown(String),
}

#[cfg(feature = "postgres")]
impl From<quench_pg::Error> for DbError {
    fn from(err: quench_pg::Error) -> Self {
        match err {
            quench_pg::Error::RowCount { got: 0, .. } => Self::NotFound,
            quench_pg::Error::Io(_)
            | quench_pg::Error::Closed
            | quench_pg::Error::Timeout(_)
            | quench_pg::Error::Tls(_)
            | quench_pg::Error::Auth(_)
            | quench_pg::Error::Config(_) => Self::ConnectionError(err.to_string()),
            quench_pg::Error::Decode(_) => Self::SerializationError(err.to_string()),
            _ => Self::QueryError(err.to_string()),
        }
    }
}

impl From<quench_serde::Error> for DbError {
    fn from(err: quench_serde::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}
