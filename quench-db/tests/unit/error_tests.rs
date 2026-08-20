//! Unit tests for `error.rs`.

use quench_db::DbError;

#[test]
fn display_messages_match_the_declared_format() {
    assert_eq!(
        DbError::ConnectionError("refused".to_string()).to_string(),
        "Database connection error: refused"
    );
    assert_eq!(
        DbError::QueryError("bad sql".to_string()).to_string(),
        "Query execution error: bad sql"
    );
    assert_eq!(
        DbError::SerializationError("bad json".to_string()).to_string(),
        "Serialization error: bad json"
    );
    assert_eq!(DbError::NotFound.to_string(), "Record not found");
    assert_eq!(
        DbError::Unknown("mystery".to_string()).to_string(),
        "Unknown database error: mystery"
    );
}

#[test]
fn sqlx_row_not_found_maps_to_not_found() {
    let err: DbError = sqlx::Error::RowNotFound.into();
    assert!(matches!(err, DbError::NotFound));
}

#[test]
fn sqlx_io_error_maps_to_connection_error() {
    let io = std::io::Error::other("connection reset");
    let err: DbError = sqlx::Error::Io(io).into();
    assert!(matches!(err, DbError::ConnectionError(_)));
}

#[test]
fn other_sqlx_errors_map_to_query_error() {
    let err: DbError = sqlx::Error::Protocol("garbled frame".to_string()).into();
    assert!(matches!(err, DbError::QueryError(_)));
}

#[test]
fn serde_json_errors_map_to_serialization_error() {
    let parse_err = serde_json::from_str::<serde_json::Value>("{not json").unwrap_err();
    let err: DbError = parse_err.into();
    assert!(matches!(err, DbError::SerializationError(_)));
}
