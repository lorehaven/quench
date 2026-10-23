//! Against a real PostgreSQL: `DB_TEST_POSTGRES_URL` (see `scripts/test-env.fish`; `foreman db` starts one).
//! Without it every test here returns early.

#![allow(clippy::significant_drop_tightening)] // tests hold guards for their whole body on purpose

use quench_core::time::Time;
use quench_core::uuid::Uuid;
use quench_pg::{Config, Connection, Error, PgArray, Pool, PoolOptions};
use quench_serde::{Value, json};
use std::time::Duration;

fn url() -> Option<String> {
    let url = std::env::var("DB_TEST_POSTGRES_URL")
        .ok()
        .filter(|u| !u.is_empty());
    if url.is_none() {
        eprintln!("skipped: DB_TEST_POSTGRES_URL is not set");
    }
    url
}

macro_rules! live {
    () => {
        match url() {
            Some(url) => url,
            None => return,
        }
    };
}

async fn connect(url: &str) -> Connection {
    Connection::connect(&Config::from_url(url).unwrap())
        .await
        .unwrap()
}

fn unique(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn connects_and_reports_server_details() {
    let url = live!();
    let mut conn = connect(&url).await;
    assert!(!conn.is_closed());
    assert_eq!(conn.transaction_status(), b'I');
    assert!(conn.process_id() > 0);
    assert!(
        conn.parameter("server_version")
            .is_some_and(|v| !v.is_empty())
    );
    assert_eq!(conn.parameter("TimeZone"), Some("UTC"));
    assert_eq!(conn.parameter("client_encoding"), Some("UTF8"));
    conn.ping().await.unwrap();
    assert!(format!("{conn:?}").contains("Connection"));
    assert!(conn.created_at() <= conn.last_used());
    conn.close().await;
}

#[tokio::test]
async fn parameters_and_results_round_trip_by_type() {
    let url = live!();
    let mut conn = connect(&url).await;
    let uuid = Uuid::new_v4();
    let time = Time::from_unix_nanos(1_792_719_480, 123_456_000);
    let row = conn
        .query_one(
            "SELECT $1::text AS t, $2::int4 AS i, $3::int8 AS big, $4::bool AS b, $5::float8 AS f, $6::numeric AS n, $7::jsonb AS j, $8::timestamptz AS ts, $9::uuid AS u, $10::bytea AS raw, $11::text[] AS arr, $12::text AS missing",
            &[&"héllo \"quoted\" \\ text", &-42i32, &9_000_000_000i64, &true, &2.5f64, &"12.50", &json!({"a": [1, 2, {"b": null}]}), &time, &uuid, &vec![0u8, 255, 16], &PgArray(vec!["x", "y z", "q\"r"]), &None::<String>],
        )
        .await
        .unwrap();
    assert_eq!(
        row.get::<String, _>("t").unwrap(),
        "héllo \"quoted\" \\ text"
    );
    assert_eq!(row.get::<i32, _>("i").unwrap(), -42);
    assert_eq!(row.get::<i64, _>("big").unwrap(), 9_000_000_000);
    assert!(row.get::<bool, _>("b").unwrap());
    assert!((row.get::<f64, _>("f").unwrap() - 2.5).abs() < f64::EPSILON);
    assert!((row.get::<f64, _>("n").unwrap() - 12.5).abs() < f64::EPSILON);
    assert_eq!(
        row.get::<Value, _>("j").unwrap(),
        json!({"a": [1, 2, {"b": null}]})
    );
    assert_eq!(row.get::<Time, _>("ts").unwrap(), time);
    assert_eq!(row.get::<Uuid, _>("u").unwrap(), uuid);
    assert_eq!(row.get::<Vec<u8>, _>("raw").unwrap(), [0, 255, 16]);
    assert_eq!(row.value("arr").unwrap(), json!(["x", "y z", "q\"r"]));
    assert_eq!(row.get::<Option<String>, _>("missing").unwrap(), None);
    assert_eq!(row.to_value()["i"], json!(-42));
    conn.close().await;
}

#[tokio::test]
async fn timestamps_are_utc_whatever_the_server_default() {
    let url = live!();
    let mut conn = connect(&url).await;
    let row = conn.query_one("SELECT TIMESTAMPTZ '2026-10-23 03:38:00+02' AS a, TIMESTAMP '2026-10-23 01:38:00' AS b, now() AS c, DATE '2026-10-23' AS d", &[]).await.unwrap();
    assert_eq!(
        row.get::<Time, _>("a").unwrap().to_rfc3339(),
        "2026-10-23T01:38:00Z"
    );
    assert_eq!(
        row.get::<Time, _>("b").unwrap().to_rfc3339(),
        "2026-10-23T01:38:00Z"
    );
    assert!(row.get::<Time, _>("c").unwrap().timestamp() > 1_700_000_000);
    assert_eq!(row.value("d").unwrap(), json!("2026-10-23"));
    conn.close().await;
}

#[tokio::test]
async fn every_numeric_type_converts() {
    let url = live!();
    let mut conn = connect(&url).await;
    let row = conn.query_one("SELECT 1::int2 a, 2::int4 b, 3::int8 c, 4.5::float4 d, 5.25::float8 e, 6::numeric f, 7.125::numeric g, 'NaN'::float8 h, 5::oid i", &[]).await.unwrap();
    assert_eq!(
        row.to_array(),
        json!([1, 2, 3, 4.5, 5.25, 6, 7.125, "NaN", 5])
    );
    conn.close().await;
}

#[tokio::test]
async fn execute_counts_rows_and_constraints_report_codes() {
    let url = live!();
    let table = unique("t_exec");
    let mut conn = connect(&url).await;
    conn.batch_execute(&format!(
        "CREATE TABLE {table} (id int PRIMARY KEY, name text NOT NULL)"
    ))
    .await
    .unwrap();
    assert_eq!(
        conn.execute(
            &format!("INSERT INTO {table} VALUES ($1, $2), ($3, $4)"),
            &[&1i32, &"a", &2i32, &"b"]
        )
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        conn.execute(
            &format!("UPDATE {table} SET name = name || '!' WHERE id <= $1"),
            &[&2i32]
        )
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        conn.execute(&format!("DELETE FROM {table} WHERE id = $1"), &[&99i32])
            .await
            .unwrap(),
        0
    );

    let dup = conn
        .execute(
            &format!("INSERT INTO {table} VALUES ($1, $2)"),
            &[&1i32, &"again"],
        )
        .await
        .unwrap_err();
    assert!(dup.is_unique_violation(), "{dup}");
    let server = dup.server().unwrap();
    assert_eq!(server.severity, "ERROR");
    assert_eq!(server.table.as_deref(), Some(table.as_str()));
    assert!(
        server
            .constraint
            .as_deref()
            .is_some_and(|c| c.ends_with("_pkey"))
    );
    assert!(
        server
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("already exists"))
    );

    let null = conn
        .execute(&format!("INSERT INTO {table} VALUES (3, NULL)"), &[])
        .await
        .unwrap_err();
    assert_eq!(null.code(), Some("23502"));

    // The connection recovers after each error.
    let syntax = conn.query("SELEC 1", &[]).await.unwrap_err();
    assert_eq!(syntax.code(), Some("42601"));
    assert!(syntax.server().unwrap().position.is_some());
    let missing = conn
        .query("SELECT * FROM no_such_table_here", &[])
        .await
        .unwrap_err();
    assert_eq!(missing.code(), Some("42P01"));
    assert!(!conn.is_closed());
    let rows = conn
        .query(&format!("SELECT id, name FROM {table} ORDER BY id"), &[])
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].deserialize_tuple::<(i32, String)>().unwrap(),
        (1, "a!".to_string())
    );
    conn.batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
    conn.close().await;
}

#[tokio::test]
async fn row_count_helpers() {
    let url = live!();
    let mut conn = connect(&url).await;
    assert!(conn.query_one("SELECT 1 WHERE false", &[]).await.is_err());
    assert!(matches!(
        conn.query_one("SELECT generate_series(1, 2)", &[]).await,
        Err(Error::RowCount { got: 2, .. })
    ));
    assert!(
        conn.query_opt("SELECT 1 WHERE false", &[])
            .await
            .unwrap()
            .is_none()
    );
    assert!(conn.query_opt("SELECT 1", &[]).await.unwrap().is_some());
    assert!(matches!(
        conn.query_opt("SELECT generate_series(1, 3)", &[]).await,
        Err(Error::RowCount { got: 3, .. })
    ));
    assert_eq!(
        conn.query("SELECT 1 WHERE false", &[]).await.unwrap().len(),
        0
    );
    conn.close().await;
}

#[tokio::test]
async fn scripts_with_several_statements_run_through_the_simple_protocol() {
    let url = live!();
    let table = unique("t_script");
    let mut conn = connect(&url).await;
    // This is the shape of a migration: several statements in one call, which a prepared statement refuses.
    conn.batch_execute(&format!("CREATE TABLE {table} (id int); INSERT INTO {table} VALUES (1), (2); CREATE INDEX ON {table} (id)")).await.unwrap();
    let results = conn.simple_query(&format!("SELECT count(*) AS n FROM {table}; SELECT id FROM {table} ORDER BY id DESC; UPDATE {table} SET id = id + 10")).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].rows[0].get::<i64, _>("n").unwrap(), 2);
    assert_eq!(
        results[1]
            .rows
            .iter()
            .map(|r| r.get::<i32, _>(&0usize).unwrap())
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(
        (results[2].tag.as_str(), results[2].rows_affected()),
        ("UPDATE 2", 2)
    );
    let prepared = conn
        .execute("CREATE TABLE x1 (a int); CREATE TABLE x2 (a int)", &[])
        .await
        .unwrap_err();
    assert!(
        prepared.to_string().contains("multiple commands"),
        "{prepared}"
    );

    // An error in the middle of a script stops it; earlier statements already ran.
    let partial = conn
        .batch_execute(&format!(
            "INSERT INTO {table} VALUES (100); SELECT 1/0; INSERT INTO {table} VALUES (101)"
        ))
        .await
        .unwrap_err();
    assert_eq!(partial.code(), Some("22012"));
    let counts = conn
        .query_one(
            &format!("SELECT count(*) FROM {table} WHERE id IN (100, 101)"),
            &[],
        )
        .await
        .unwrap();
    assert_eq!(
        counts.get::<i64, _>(&0usize).unwrap(),
        0,
        "the simple protocol runs a multi-statement script in one implicit transaction"
    );
    assert_eq!(
        conn.simple_query("").await.unwrap().len(),
        1,
        "an empty query is an empty result"
    );
    conn.batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
    conn.close().await;
}

#[tokio::test]
async fn transactions_commit_roll_back_and_clean_up_after_themselves() {
    let url = live!();
    let table = unique("t_tx");
    let mut conn = connect(&url).await;
    conn.batch_execute(&format!("CREATE TABLE {table} (id int)"))
        .await
        .unwrap();

    let mut tx = conn.begin().await.unwrap();
    tx.execute(&format!("INSERT INTO {table} VALUES ($1)"), &[&1i32])
        .await
        .unwrap();
    assert_eq!(
        tx.query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .await
            .unwrap()
            .get::<i64, _>(&0usize)
            .unwrap(),
        1
    );
    assert!(
        tx.query_opt(&format!("SELECT id FROM {table}"), &[])
            .await
            .unwrap()
            .is_some()
    );
    tx.batch_execute(&format!("INSERT INTO {table} VALUES (2)"))
        .await
        .unwrap();
    assert_eq!(
        tx.simple_query(&format!("SELECT id FROM {table}"))
            .await
            .unwrap()[0]
            .rows
            .len(),
        2
    );
    tx.commit().await.unwrap();
    assert_eq!(conn.transaction_status(), b'I');

    let mut tx = conn.begin().await.unwrap();
    tx.execute(&format!("INSERT INTO {table} VALUES (3)"), &[])
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    {
        let mut tx = conn.begin().await.unwrap();
        tx.execute(&format!("INSERT INTO {table} VALUES (4)"), &[])
            .await
            .unwrap();
        // Dropped without commit.
    }
    assert_eq!(
        conn.transaction_status(),
        b'T',
        "the server still has the transaction open"
    );
    let count = conn
        .query_one(&format!("SELECT count(*) FROM {table}"), &[])
        .await
        .unwrap();
    assert_eq!(
        count.get::<i64, _>(&0usize).unwrap(),
        2,
        "the dropped guard was rolled back before the next statement"
    );
    assert_eq!(conn.transaction_status(), b'I');

    // An error inside a transaction fails the rest of it until rollback.
    let mut tx = conn.begin().await.unwrap();
    assert!(tx.execute("SELECT 1/0", &[]).await.is_err());
    let after = tx.execute("SELECT 1", &[]).await.unwrap_err();
    assert_eq!(after.code(), Some("25P02"));
    tx.rollback().await.unwrap();
    conn.execute("SELECT 1", &[]).await.unwrap();

    conn.batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
    conn.close().await;
}

#[tokio::test]
async fn notices_and_notifications_do_not_disturb_a_conversation() {
    let url = live!();
    let mut conn = connect(&url).await;
    conn.batch_execute("DO $$ BEGIN RAISE NOTICE 'hello from the server'; END $$")
        .await
        .unwrap();
    conn.batch_execute("LISTEN quench_test_channel; NOTIFY quench_test_channel, 'ping'")
        .await
        .unwrap();
    assert_eq!(
        conn.query_one("SELECT 1 AS one", &[])
            .await
            .unwrap()
            .get::<i32, _>("one")
            .unwrap(),
        1
    );
    conn.close().await;
}

#[tokio::test]
async fn bad_credentials_and_databases_are_server_errors() {
    let url = live!();
    let mut config = Config::from_url(&url).unwrap();
    config.password = Some("definitely-wrong".to_string());
    let err = Connection::connect(&config).await.unwrap_err();
    assert_eq!(err.code(), Some("28P01"), "{err}");

    let mut config = Config::from_url(&url).unwrap();
    config.database = Some("no_such_database_xyz".to_string());
    let err = Connection::connect(&config).await.unwrap_err();
    assert_eq!(err.code(), Some("3D000"), "{err}");

    let mut config = Config::from_url(&url).unwrap();
    config.password = None;
    let err = Connection::connect(&config).await.unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err}");

    let mut config = Config::from_url(&url).unwrap();
    config.port = 1;
    assert!(matches!(
        Connection::connect(&config).await.unwrap_err(),
        Error::Io(_)
    ));
}

#[cfg(feature = "tls")]
#[tokio::test]
async fn tls_is_negotiated_when_asked_and_refused_when_the_server_has_none() {
    let url = live!();
    let config = Config::from_url(&format!("{url}?sslmode=require")).unwrap();
    match Connection::connect(&config).await {
        Ok(mut conn) => {
            // A server with TLS: the connection works and is encrypted.
            let row = conn
                .query_one(
                    "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()",
                    &[],
                )
                .await
                .unwrap();
            assert!(row.get::<bool, _>("ssl").unwrap());
        }
        Err(err) => assert!(
            matches!(&err, Error::Tls(m) if m.contains("does not support TLS")),
            "{err}"
        ),
    }
    let prefer = Config::from_url(&format!("{url}?sslmode=prefer")).unwrap();
    connect_with(&prefer).await;
    let disable = Config::from_url(&format!("{url}?sslmode=disable")).unwrap();
    connect_with(&disable).await;
}

#[cfg(feature = "tls")]
async fn connect_with(config: &Config) {
    let mut conn = Connection::connect(config).await.unwrap();
    conn.ping().await.unwrap();
}

#[tokio::test]
async fn statement_timeouts_abandon_the_connection() {
    let url = live!();
    let mut config = Config::from_url(&url).unwrap();
    config.statement_timeout = Some(Duration::from_millis(200));
    let mut conn = Connection::connect(&config).await.unwrap();
    let err = conn.query("SELECT pg_sleep(5)", &[]).await.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err}");
    assert!(
        conn.is_closed(),
        "a half-finished conversation cannot be resumed"
    );
    assert!(matches!(
        conn.query("SELECT 1", &[]).await,
        Err(Error::Closed)
    ));
    let mut conn = Connection::connect(&config).await.unwrap();
    assert!(matches!(
        conn.simple_query("SELECT pg_sleep(5)").await,
        Err(Error::Timeout(_))
    ));
}

#[tokio::test]
async fn cancelling_a_statement_leaves_the_connection_unusable_not_confused() {
    let url = live!();
    let mut conn = connect(&url).await;
    let outcome = tokio::time::timeout(
        Duration::from_millis(100),
        conn.query("SELECT pg_sleep(3)", &[]),
    )
    .await;
    assert!(
        outcome.is_err(),
        "the sleep outlasts the timeout and the future is dropped"
    );
    assert!(conn.is_closed());
    assert!(matches!(
        conn.query("SELECT 1", &[]).await,
        Err(Error::Closed)
    ));
}

#[tokio::test]
async fn the_pool_reuses_connections_and_limits_their_number() {
    let url = live!();
    let options = PoolOptions {
        max_size: 2,
        acquire_timeout: Duration::from_millis(300),
        ..PoolOptions::default()
    };
    let pool = Pool::connect_with(Config::from_url(&url).unwrap(), options)
        .await
        .unwrap();
    assert_eq!(
        pool.idle_count(),
        1,
        "connect_with opened and returned one connection"
    );
    let first_pid = {
        let conn = pool.get().await.unwrap();
        conn.process_id()
    };
    let second_pid = pool.get().await.unwrap().process_id();
    assert_eq!(first_pid, second_pid, "the idle connection is reused");

    let a = pool.get().await.unwrap();
    let b = pool.get().await.unwrap();
    assert_ne!(a.process_id(), b.process_id());
    assert_eq!(pool.available(), 0);
    let err = pool.get().await.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err}");
    drop(a);
    assert_eq!(pool.available(), 1);
    assert!(
        pool.get().await.is_ok(),
        "a returned connection frees the slot"
    );
    drop(b);
    assert!(format!("{pool:?}").contains("Pool"));
    pool.close().await;
    assert_eq!(pool.idle_count(), 0);
}

#[tokio::test]
async fn pooled_helpers_and_concurrent_use() {
    let url = live!();
    let table = unique("t_pool");
    let pool = Pool::connect(&url).await.unwrap();
    pool.batch_execute(&format!(
        "CREATE TABLE {table} (id int PRIMARY KEY, label text)"
    ))
    .await
    .unwrap();
    let mut tasks = Vec::new();
    for i in 0..20i32 {
        let pool = pool.clone();
        let table = table.clone();
        tasks.push(tokio::spawn(async move {
            pool.execute(
                &format!("INSERT INTO {table} VALUES ($1, $2)"),
                &[&i, &format!("row {i}")],
            )
            .await
            .unwrap();
            pool.query_one(&format!("SELECT label FROM {table} WHERE id = $1"), &[&i])
                .await
                .unwrap()
                .get::<String, _>("label")
                .unwrap()
        }));
    }
    for (i, task) in tasks.into_iter().enumerate() {
        assert_eq!(task.await.unwrap(), format!("row {i}"));
    }
    assert_eq!(
        pool.query(&format!("SELECT * FROM {table}"), &[])
            .await
            .unwrap()
            .len(),
        20
    );
    assert!(
        pool.query_opt(&format!("SELECT * FROM {table} WHERE id = -1"), &[])
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        pool.simple_query(&format!("SELECT count(*) FROM {table}"))
            .await
            .unwrap()[0]
            .rows[0]
            .get::<i64, _>(&0usize)
            .unwrap(),
        20
    );
    pool.batch_execute(&format!("DROP TABLE {table}"))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_pool_discards_connections_that_are_not_clean() {
    let url = live!();
    let pool = Pool::connect_with(
        Config::from_url(&url).unwrap(),
        PoolOptions {
            max_size: 1,
            ..PoolOptions::default()
        },
    )
    .await
    .unwrap();
    let pid_in_transaction = {
        let mut conn = pool.get().await.unwrap();
        let pid = conn.process_id();
        conn.batch_execute("BEGIN").await.unwrap();
        pid
    };
    assert_eq!(
        pool.idle_count(),
        0,
        "a connection returned inside a transaction is dropped"
    );
    let fresh = pool.get().await.unwrap();
    assert_ne!(fresh.process_id(), pid_in_transaction);
    drop(fresh);

    // A cancelled statement also costs the connection.
    {
        let mut conn = pool.get().await.unwrap();
        let _ = tokio::time::timeout(
            Duration::from_millis(50),
            conn.query("SELECT pg_sleep(2)", &[]),
        )
        .await;
    }
    assert_eq!(pool.idle_count(), 0);
    assert!(pool.get().await.unwrap().ping().await.is_ok());
}

#[tokio::test]
async fn pooled_connections_run_transactions() {
    let url = live!();
    let pool = Pool::connect(&url).await.unwrap();
    let mut conn = pool.get().await.unwrap();
    let tx = conn.begin().await.unwrap();
    tx.rollback().await.unwrap();
    assert!(format!("{conn:?}").contains("PooledConnection"));
}

#[tokio::test]
async fn idle_connections_that_were_killed_are_replaced() {
    let url = live!();
    let pool = Pool::connect_with(
        Config::from_url(&url).unwrap(),
        PoolOptions {
            max_size: 2,
            test_after_idle: Duration::ZERO,
            ..PoolOptions::default()
        },
    )
    .await
    .unwrap();
    let victim = pool.get().await.unwrap().process_id();
    // Terminate the idle backend from another connection, then check out again.
    let mut killer = connect(&url).await;
    killer
        .query(
            "SELECT pg_terminate_backend($1)",
            &[&i32::try_from(victim).unwrap()],
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut again = pool.get().await.unwrap();
    assert_ne!(again.process_id(), victim);
    again.ping().await.unwrap();
}

#[tokio::test]
async fn pools_can_start_lazily() {
    let url = live!();
    let pool = Pool::lazy(Config::from_url(&url).unwrap(), PoolOptions::default());
    assert_eq!(pool.idle_count(), 0);
    pool.query_one("SELECT 1", &[]).await.unwrap();
    assert_eq!(pool.idle_count(), 1);
    let bad = Pool::lazy(
        Config {
            port: 1,
            ..Config::from_url(&url).unwrap()
        },
        PoolOptions::default(),
    );
    assert!(bad.get().await.is_err());
    assert!(Pool::connect("not a url").await.is_err());
}

#[tokio::test]
async fn max_lifetime_retires_connections_on_return() {
    let url = live!();
    let pool = Pool::connect_with(
        Config::from_url(&url).unwrap(),
        PoolOptions {
            max_size: 1,
            max_lifetime: Duration::ZERO,
            ..PoolOptions::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        pool.idle_count(),
        0,
        "the first connection was already past its lifetime when it came back"
    );
    pool.get().await.unwrap();
    assert_eq!(pool.idle_count(), 0);
}

#[tokio::test]
async fn idle_timeout_skips_stale_connections() {
    let url = live!();
    let pool = Pool::connect_with(
        Config::from_url(&url).unwrap(),
        PoolOptions {
            max_size: 1,
            idle_timeout: Duration::ZERO,
            ..PoolOptions::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(pool.idle_count(), 1);
    tokio::time::sleep(Duration::from_millis(5)).await;
    let conn = pool.get().await.unwrap();
    assert_eq!(pool.idle_count(), 0);
    drop(conn);
}
