//! Against a real Redis: `CACHE_TEST_REDIS_URL` (see `scripts/test-env.fish`; `foreman db` starts one).
//! Without it every test here returns early.

use quench_core::uuid::Uuid;
use quench_redis::{Client, Config, Connection, Error, PoolOptions, Redis, Value, cmd};
use std::time::Duration;

fn url() -> Option<String> {
    let url = std::env::var("CACHE_TEST_REDIS_URL")
        .ok()
        .filter(|u| !u.is_empty());
    if url.is_none() {
        eprintln!("skipped: CACHE_TEST_REDIS_URL is not set");
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

fn key(prefix: &str) -> String {
    format!("quench-test:{prefix}:{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn strings_numbers_and_expiry() {
    let url = live!();
    let redis = Redis::connect(&url).await.unwrap();
    assert!(!redis.is_cluster());
    let k = key("strings");
    redis
        .exec::<()>(&cmd("SET").arg(&k).arg("hello"))
        .await
        .unwrap();
    assert_eq!(
        redis
            .exec::<Option<String>>(&cmd("GET").arg(&k))
            .await
            .unwrap()
            .as_deref(),
        Some("hello")
    );
    assert_eq!(
        redis
            .exec::<Option<String>>(&cmd("GET").arg("quench-test:missing"))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        redis
            .exec::<i64>(&cmd("APPEND").arg(&k).arg(" world"))
            .await
            .unwrap(),
        11
    );
    assert_eq!(redis.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap(), 1);

    let counter = key("counter");
    assert_eq!(
        redis
            .exec::<i64>(&cmd("INCRBY").arg(&counter).arg(&5))
            .await
            .unwrap(),
        5
    );
    assert_eq!(
        redis.exec::<u64>(&cmd("INCR").arg(&counter)).await.unwrap(),
        6
    );
    assert!(
        (redis
            .exec::<f64>(&cmd("INCRBYFLOAT").arg(&counter).arg(&0.5f64))
            .await
            .unwrap()
            - 6.5)
            .abs()
            < f64::EPSILON
    );

    redis
        .exec::<()>(&cmd("SET").arg(&counter).arg("x").arg("EX").arg(&100))
        .await
        .unwrap();
    let ttl: i64 = redis.exec(&cmd("TTL").arg(&counter)).await.unwrap();
    assert!((1..=100).contains(&ttl));
    let existed: bool = redis
        .exec(&cmd("EXPIRE").arg(&counter).arg(&1))
        .await
        .unwrap();
    assert!(existed);
    assert_eq!(
        redis
            .exec::<Option<String>>(&cmd("GETDEL").arg("quench-test:missing"))
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn binary_and_large_values_round_trip() {
    let url = live!();
    let redis = Redis::connect(&url).await.unwrap();
    let k = key("binary");
    let blob: Vec<u8> = (0..=255u8).cycle().take(1_500_000).collect();
    redis
        .exec::<()>(&cmd("SET").arg(&k).arg(&blob))
        .await
        .unwrap();
    let back: Vec<u8> = redis.exec(&cmd("GET").arg(&k)).await.unwrap();
    assert_eq!(back, blob);
    redis.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap();
    let tricky = "line1\r\nline2 \u{1F980} \"quoted\"";
    redis
        .exec::<()>(&cmd("SET").arg(&k).arg(tricky))
        .await
        .unwrap();
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&k)).await.unwrap(),
        tricky
    );
    redis.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap();
}

#[tokio::test]
async fn sets_lists_and_scan() {
    let url = live!();
    let redis = Redis::connect(&url).await.unwrap();
    let set = key("set");
    assert_eq!(
        redis
            .exec::<i64>(&cmd("SADD").arg(&set).args(&["a", "b", "c"]))
            .await
            .unwrap(),
        3
    );
    let mut members: Vec<String> = redis.exec(&cmd("SMEMBERS").arg(&set)).await.unwrap();
    members.sort();
    assert_eq!(members, ["a", "b", "c"]);
    assert_eq!(
        redis
            .exec::<i64>(&cmd("SREM").arg(&set).arg("a"))
            .await
            .unwrap(),
        1
    );

    let list = key("list");
    redis
        .exec::<i64>(&cmd("RPUSH").arg(&list).args(&[1, 2, 3]))
        .await
        .unwrap();
    assert_eq!(
        redis
            .exec::<Vec<i64>>(&cmd("LRANGE").arg(&list).arg(&0).arg(&-1))
            .await
            .unwrap(),
        [1, 2, 3]
    );

    let prefix = key("scan");
    for i in 0..30 {
        redis
            .exec::<()>(&cmd("SET").arg(&format!("{prefix}:{i}")).arg("v"))
            .await
            .unwrap();
    }
    let mut cursor = 0u64;
    let mut found = 0;
    loop {
        let (next, keys): (u64, Vec<String>) = redis
            .exec(
                &cmd("SCAN")
                    .arg(&cursor)
                    .arg("MATCH")
                    .arg(&format!("{prefix}:*"))
                    .arg("COUNT")
                    .arg(&10),
            )
            .await
            .unwrap();
        found += keys.len();
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
    assert_eq!(found, 30);
    for i in 0..30 {
        redis
            .exec::<i64>(&cmd("DEL").arg(&format!("{prefix}:{i}")))
            .await
            .unwrap();
    }
    redis
        .exec::<i64>(&cmd("DEL").arg(&set).arg(&list))
        .await
        .unwrap();
}

#[tokio::test]
async fn server_errors_are_typed_and_the_connection_survives() {
    let url = live!();
    let redis = Redis::connect(&url).await.unwrap();
    let k = key("wrongtype");
    redis
        .exec::<()>(&cmd("SET").arg(&k).arg("text"))
        .await
        .unwrap();
    let err = redis
        .exec::<Vec<String>>(&cmd("SMEMBERS").arg(&k))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Server(s) if s.kind == "WRONGTYPE"),
        "{err}"
    );
    assert!(
        matches!(redis.exec::<()>(&cmd("NOSUCHCOMMAND")).await, Err(Error::Server(s)) if s.kind == "ERR")
    );
    assert_eq!(
        redis.exec::<String>(&cmd("GET").arg(&k)).await.unwrap(),
        "text"
    );
    assert!(
        matches!(
            redis.exec::<i64>(&cmd("GET").arg(&k)).await,
            Err(Error::Type(_))
        ),
        "a reply that does not fit the requested type"
    );
    redis.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap();
}

#[tokio::test]
async fn pipelines_return_every_reply_including_errors() {
    let url = live!();
    let client = Client::connect(&url).await.unwrap();
    let k = key("pipe");
    let replies = client
        .pipeline(&[
            cmd("SET").arg(&k).arg("1"),
            cmd("INCR").arg(&k),
            cmd("SADD").arg(&k).arg("x"),
            cmd("GET").arg(&k),
        ])
        .await
        .unwrap();
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[0], Value::Simple("OK".into()));
    assert_eq!(replies[1], Value::Int(2));
    assert!(matches!(&replies[2], Value::Error(e) if e.kind == "WRONGTYPE"));
    assert_eq!(replies[3], Value::Bulk(b"2".to_vec()));
    assert!(client.pipeline(&[]).await.unwrap().is_empty());
    client.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap();
    assert!(format!("{client:?}").contains("Client"));
}

#[tokio::test]
async fn resp3_replies_decode() {
    let url = live!();
    let config = Config::from_url(&url).unwrap();
    let mut conn = Connection::connect(&config).await.unwrap();
    let hello = conn.request(&cmd("HELLO").arg(&3)).await.unwrap();
    assert!(matches!(hello, Value::Map(_)), "{hello:?}");
    let hash = key("hash");
    conn.request(&cmd("HSET").arg(&hash).arg("a").arg("1").arg("b").arg("2"))
        .await
        .unwrap();
    let Value::Map(entries) = conn.request(&cmd("HGETALL").arg(&hash)).await.unwrap() else {
        panic!("a map")
    };
    assert_eq!(entries.len(), 2);
    let set = key("set3");
    conn.request(&cmd("SADD").arg(&set).arg("m")).await.unwrap();
    assert!(matches!(
        conn.request(&cmd("SMEMBERS").arg(&set)).await.unwrap(),
        Value::Set(_)
    ));
    assert!(matches!(
        conn.request(&cmd("INCRBYFLOAT").arg(&key("f")).arg("1.5"))
            .await
            .unwrap(),
        Value::Bulk(_)
    ));
    assert_eq!(
        conn.request(&cmd("GET").arg("quench-test:nothing-here"))
            .await
            .unwrap(),
        Value::Nil
    );
    assert_eq!(
        conn.request(&cmd("EXISTS").arg(&hash)).await.unwrap(),
        Value::Int(1)
    );
    conn.request(&cmd("DEL").arg(&hash).arg(&set))
        .await
        .unwrap();
    conn.ping().await.unwrap();
    assert!(!conn.looks_dead());
    assert!(conn.created_at() <= conn.last_used());
    assert!(format!("{conn:?}").contains("Connection"));
    conn.close().await;
}

#[tokio::test]
async fn databases_and_authentication() {
    let url = live!();
    let on_db1 = Config {
        db: 1,
        ..Config::from_url(&url).unwrap()
    };
    let k = key("db1");
    let mut one = Connection::connect(&on_db1).await.unwrap();
    one.request(&cmd("SET").arg(&k).arg("in db 1"))
        .await
        .unwrap();
    let mut zero = Connection::connect(&Config::from_url(&url).unwrap())
        .await
        .unwrap();
    assert_eq!(zero.request(&cmd("GET").arg(&k)).await.unwrap(), Value::Nil);
    one.request(&cmd("DEL").arg(&k)).await.unwrap();

    // This server has no password configured, so asking to AUTH is an error the server explains.
    let with_password = Config {
        password: Some("nope".into()),
        ..Config::from_url(&url).unwrap()
    };
    assert!(matches!(
        Connection::connect(&with_password).await,
        Err(Error::Server(_))
    ));
    let with_user = Config {
        username: Some("default".into()),
        password: Some("nope".into()),
        ..Config::from_url(&url).unwrap()
    };
    // The `default` user has no password (`nopass`), so ACL-style AUTH accepts anything.
    assert!(Connection::connect(&with_user).await.is_ok());
    let unknown_user = Config {
        username: Some("nobody".into()),
        password: Some("x".into()),
        ..Config::from_url(&url).unwrap()
    };
    assert!(
        matches!(Connection::connect(&unknown_user).await, Err(Error::Server(s)) if s.kind == "WRONGPASS")
    );
}

#[tokio::test]
async fn dead_connections_are_replaced_transparently() {
    let url = live!();
    let client = Client::connect(&url).await.unwrap();
    // Find the id of the pooled connection, then have another client kill exactly that one.
    let id: i64 = client.exec(&cmd("CLIENT").arg("ID")).await.unwrap();
    assert_eq!(client.idle_count(), 1);
    let assassin = Client::connect(&url).await.unwrap();
    let killed: i64 = assassin
        .exec(&cmd("CLIENT").arg("KILL").arg("ID").arg(&id))
        .await
        .unwrap();
    assert_eq!(killed, 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(client.exec::<String>(&cmd("PING")).await.unwrap(), "PONG");
    client.close().await;
    assert_eq!(client.idle_count(), 0);
}

#[tokio::test]
async fn command_timeouts_discard_the_connection() {
    let url = live!();
    let config = Config {
        command_timeout: Some(Duration::from_millis(150)),
        ..Config::from_url(&url).unwrap()
    };
    let mut conn = Connection::connect(&config).await.unwrap();
    let err = conn
        .request(&cmd("BLPOP").arg(&key("never")).arg(&5))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err}");
    assert!(conn.is_closed());
    assert!(matches!(
        conn.request(&cmd("PING")).await,
        Err(Error::Closed)
    ));

    // A pooled client does not hand the broken connection out again.
    let client = Client::connect_with(config, PoolOptions::default())
        .await
        .unwrap();
    assert!(
        client
            .exec::<()>(&cmd("BLPOP").arg(&key("never")).arg(&5))
            .await
            .is_err()
    );
    assert_eq!(client.exec::<String>(&cmd("PING")).await.unwrap(), "PONG");
}

#[tokio::test]
async fn cancelling_a_command_leaves_the_connection_unusable() {
    let url = live!();
    let mut conn = Connection::connect(&Config::from_url(&url).unwrap())
        .await
        .unwrap();
    let outcome = tokio::time::timeout(
        Duration::from_millis(100),
        conn.request(&cmd("BLPOP").arg(&key("never")).arg(&3)),
    )
    .await;
    assert!(outcome.is_err());
    assert!(conn.is_closed());
}

#[tokio::test]
async fn the_pool_limits_and_reuses_connections() {
    let url = live!();
    let client = Client::connect_with(
        Config::from_url(&url).unwrap(),
        PoolOptions {
            max_size: 2,
            acquire_timeout: Duration::from_millis(200),
            ..PoolOptions::default()
        },
    )
    .await
    .unwrap();
    let a = client.get().await.unwrap();
    let b = client.get().await.unwrap();
    assert!(matches!(client.get().await, Err(Error::Timeout(_))));
    drop(a);
    assert!(client.get().await.is_ok());
    drop(b);
    assert!(format!("{:?}", client.get().await.unwrap()).contains("PooledConnection"));
    assert_eq!(client.address(), Config::from_url(&url).unwrap().address());

    let mut tasks = Vec::new();
    for i in 0..40 {
        let client = client.clone();
        tasks.push(tokio::spawn(async move {
            let k = format!("quench-test:concurrent:{i}");
            client
                .exec::<()>(&cmd("SET").arg(&k).arg(&i))
                .await
                .unwrap();
            let back: i64 = client.exec(&cmd("GET").arg(&k)).await.unwrap();
            client.exec::<i64>(&cmd("DEL").arg(&k)).await.unwrap();
            back
        }));
    }
    for (i, task) in tasks.into_iter().enumerate() {
        assert_eq!(task.await.unwrap(), i64::try_from(i).unwrap());
    }
}

#[tokio::test]
async fn lazy_clients_and_unreachable_servers() {
    let url = live!();
    let lazy = Client::lazy(Config::from_url(&url).unwrap(), PoolOptions::default());
    assert_eq!(lazy.idle_count(), 0);
    lazy.exec::<String>(&cmd("PING")).await.unwrap();
    assert_eq!(lazy.idle_count(), 1);
    let nowhere = Config {
        port: 1,
        connect_timeout: Duration::from_millis(300),
        ..Config::from_url(&url).unwrap()
    };
    assert!(matches!(
        Client::connect_with(nowhere, PoolOptions::default()).await,
        Err(Error::Io(_) | Error::Timeout(_))
    ));
    assert!(Client::connect("memcached://h").await.is_err());
    let single = Redis::connect(&url).await.unwrap();
    assert!(single.primaries().await.unwrap().is_empty());
    assert!(
        single
            .exec_on::<String>("ignored:1", &cmd("PING"))
            .await
            .is_ok()
    );
}
