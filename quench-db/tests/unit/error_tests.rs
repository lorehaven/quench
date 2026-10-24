//! Unit tests for `error.rs`.

use quench_db::DbError;

#[quench_test::test]
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

#[cfg(feature = "postgres")]
#[quench_test::test]
fn pg_errors_map_by_kind() {
    let none: DbError = quench_pg::Error::RowCount {
        expected: "exactly one",
        got: 0,
    }
    .into();
    assert!(matches!(none, DbError::NotFound));
    let many: DbError = quench_pg::Error::RowCount {
        expected: "exactly one",
        got: 2,
    }
    .into();
    assert!(matches!(many, DbError::QueryError(_)));
    for err in [
        quench_pg::Error::Io(std::io::Error::other("connection reset")),
        quench_pg::Error::Closed,
        quench_pg::Error::Timeout("connecting"),
        quench_pg::Error::Tls("handshake".into()),
        quench_pg::Error::Auth("denied".into()),
        quench_pg::Error::Config("bad url".into()),
    ] {
        assert!(matches!(DbError::from(err), DbError::ConnectionError(_)));
    }
    assert!(matches!(
        DbError::from(quench_pg::Error::Decode("int".into())),
        DbError::SerializationError(_)
    ));
    assert!(matches!(
        DbError::from(quench_pg::Error::Protocol("odd".into())),
        DbError::QueryError(_)
    ));
    let server = quench_pg::Error::Server(Box::default());
    assert!(matches!(DbError::from(server), DbError::QueryError(_)));
}

#[quench_test::test]
fn serde_errors_map_to_serialization_error() {
    let err: DbError = quench_serde::Error::new("bad json").into();
    assert!(matches!(err, DbError::SerializationError(_)));
}
