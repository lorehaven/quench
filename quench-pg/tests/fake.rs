//! The client against a scripted server: authentication variants, TLS negotiation,
//! and the malformed input a real server never sends.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::base64::STANDARD;
use quench_crypto::hash::sha256;
use quench_crypto::hmac::{self, Algorithm};
use quench_pg::{Config, Connection, Error, SslMode};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::net::TcpListener;

fn frame(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&u32::try_from(body.len() + 4).unwrap().to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn auth(kind: u32, extra: &[u8]) -> Vec<u8> {
    let mut body = kind.to_be_bytes().to_vec();
    body.extend_from_slice(extra);
    frame(b'R', &body)
}

fn ready() -> Vec<u8> {
    frame(b'Z', b"I")
}

fn error_response(code: &str, message: &str) -> Vec<u8> {
    frame(b'E', format!("SERROR\0C{code}\0M{message}\0\0").as_bytes())
}

/// Reads the untagged startup message and returns its parameters as text.
async fn read_startup<S: AsyncReadExt + Unpin>(server: &mut S) -> String {
    let mut len = [0u8; 4];
    server.read_exact(&mut len).await.unwrap();
    let mut body = vec![0u8; u32::from_be_bytes(len) as usize - 4];
    server.read_exact(&mut body).await.unwrap();
    String::from_utf8_lossy(&body[4..]).into_owned()
}

/// Reads one tagged frontend message.
async fn read_tagged<S: AsyncReadExt + Unpin>(server: &mut S) -> (u8, Vec<u8>) {
    let mut head = [0u8; 5];
    server.read_exact(&mut head).await.unwrap();
    let mut body = vec![0u8; u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize - 4];
    server.read_exact(&mut body).await.unwrap();
    (head[0], body)
}

fn config() -> Arc<Config> {
    Arc::new(Config {
        user: "u".into(),
        password: Some("pw".into()),
        database: Some("d".into()),
        ..Config::default()
    })
}

fn pair() -> (Connection, DuplexStream) {
    let (client, server) = tokio::io::duplex(1 << 16);
    (Connection::from_stream(Box::new(client), config()), server)
}

async fn session() -> (Connection, DuplexStream) {
    let (mut conn, mut server) = pair();
    let task = tokio::spawn(async move {
        read_startup(&mut server).await;
        server
            .write_all(&[auth(0, &[]), ready()].concat())
            .await
            .unwrap();
        server
    });
    conn.startup().await.unwrap();
    (conn, task.await.unwrap())
}

#[quench_test::test]
async fn startup_sends_the_expected_parameters_and_collects_server_state() {
    let (mut conn, mut server) = pair();
    let task = tokio::spawn(async move {
        let params = read_startup(&mut server).await;
        let mut key = 77u32.to_be_bytes().to_vec();
        key.extend_from_slice(&99u32.to_be_bytes());
        server
            .write_all(
                &[
                    auth(0, &[]),
                    frame(b'S', b"server_version\x0018.1\0"),
                    frame(b'N', b"SNOTICE\0Mwelcome\0\0"),
                    frame(b'K', &key),
                    ready(),
                ]
                .concat(),
            )
            .await
            .unwrap();
        params
    });
    conn.startup().await.unwrap();
    let params = task.await.unwrap();
    assert!(
        params.contains("user\0u\0")
            && params.contains("database\0d\0")
            && params.contains("client_encoding\0UTF8\0")
            && params.contains("TimeZone\0UTC\0"),
        "{params:?}"
    );
    assert_eq!(conn.process_id(), 77);
    assert_eq!(conn.parameter("server_version"), Some("18.1"));
    assert_eq!(conn.transaction_status(), b'I');
}

#[quench_test::test]
async fn cleartext_passwords_are_sent_when_asked() {
    let (mut conn, mut server) = pair();
    let task = tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&auth(3, &[])).await.unwrap();
        let (tag, body) = read_tagged(&mut server).await;
        server
            .write_all(&[auth(0, &[]), ready()].concat())
            .await
            .unwrap();
        (tag, body)
    });
    conn.startup().await.unwrap();
    assert_eq!(task.await.unwrap(), (b'p', b"pw\0".to_vec()));
}

#[quench_test::test]
async fn unsupported_authentication_is_a_clear_error() {
    for (kind, extra, needle) in [
        (5u32, vec![1u8, 2, 3, 4], "MD5"),
        (7, vec![], "unsupported authentication method 7"),
    ] {
        let (mut conn, mut server) = pair();
        tokio::spawn(async move {
            read_startup(&mut server).await;
            server.write_all(&auth(kind, &extra)).await.unwrap();
            server
        });
        let err = conn.startup().await.unwrap_err();
        assert!(
            matches!(&err, Error::Auth(m) if m.contains(needle)),
            "{err}"
        );
    }
}

#[quench_test::test]
async fn a_missing_password_is_reported_before_anything_is_sent() {
    let (client, mut server) = tokio::io::duplex(1 << 16);
    let mut conn = Connection::from_stream(
        Box::new(client),
        Arc::new(Config {
            password: None,
            ..Config::default()
        }),
    );
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&auth(3, &[])).await.unwrap();
        server
    });
    let err = conn.startup().await.unwrap_err();
    assert!(
        matches!(&err, Error::Auth(m) if m.contains("none was configured")),
        "{err}"
    );
}

/// A server side of SCRAM that follows the protocol for `password`, optionally forging the final signature.
async fn scram_server(mut server: DuplexStream, password: &str, forge_signature: bool) {
    read_startup(&mut server).await;
    server
        .write_all(&auth(10, b"SCRAM-SHA-256\0\0"))
        .await
        .unwrap();
    let (_, initial) = read_tagged(&mut server).await;
    // mechanism\0 + i32 length + client-first
    let start = initial.iter().position(|&b| b == 0).unwrap() + 5;
    let client_first = String::from_utf8(initial[start..].to_vec()).unwrap();
    let client_first_bare = client_first.strip_prefix("n,,").unwrap().to_string();
    let client_nonce = client_first_bare.split("r=").nth(1).unwrap().to_string();

    let server_nonce = format!("{client_nonce}ServerPart");
    let salt = b"0123456789abcdef";
    let server_first = format!("r={server_nonce},s={},i=4096", STANDARD.encode(salt));
    server
        .write_all(&auth(11, server_first.as_bytes()))
        .await
        .unwrap();
    let (_, client_final) = read_tagged(&mut server).await;
    let client_final = String::from_utf8(client_final).unwrap();
    let (without_proof, proof) = client_final.rsplit_once(",p=").unwrap();

    let mut salted = [0u8; 32];
    quench_crypto::pbkdf2::derive(
        Algorithm::Sha256,
        4096,
        salt,
        password.as_bytes(),
        &mut salted,
    );
    let client_key = hmac::sign(Algorithm::Sha256, &salted, "Client Key");
    let stored = sha256(&client_key);
    let auth_message = format!("{client_first_bare},{server_first},{without_proof}");
    let signature = hmac::sign(Algorithm::Sha256, &stored, &auth_message);
    let expected_proof: Vec<u8> = client_key
        .iter()
        .zip(&signature)
        .map(|(a, b)| a ^ b)
        .collect();
    if STANDARD.decode(proof).unwrap() != expected_proof {
        server
            .write_all(&error_response("28P01", "password authentication failed"))
            .await
            .unwrap();
        return;
    }
    let server_key = hmac::sign(Algorithm::Sha256, &salted, "Server Key");
    let mut server_signature = hmac::sign(Algorithm::Sha256, &server_key, &auth_message);
    if forge_signature {
        server_signature[0] ^= 1;
    }
    let final_message = format!("v={}", STANDARD.encode(server_signature));
    server
        .write_all(&[auth(12, final_message.as_bytes()), auth(0, &[]), ready()].concat())
        .await
        .unwrap();
}

#[quench_test::test]
async fn scram_completes_against_an_independent_server_implementation() {
    let (mut conn, server) = pair();
    tokio::spawn(scram_server(server, "pw", false));
    conn.startup().await.unwrap();
}

#[quench_test::test]
async fn a_wrong_password_fails_scram() {
    let (mut conn, server) = pair();
    tokio::spawn(scram_server(server, "other", false));
    let err = conn.startup().await.unwrap_err();
    assert_eq!(err.code(), Some("28P01"), "{err}");
    assert!(conn.is_closed());
}

#[quench_test::test]
async fn a_server_that_does_not_know_the_password_is_rejected() {
    let (mut conn, server) = pair();
    tokio::spawn(scram_server(server, "pw", true));
    let err = conn.startup().await.unwrap_err();
    assert!(
        matches!(&err, Error::Auth(m) if m.contains("does not know the password")),
        "{err}"
    );
}

#[quench_test::test]
async fn scram_needs_the_right_mechanism_and_a_sane_order() {
    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server
            .write_all(&auth(10, b"SCRAM-SHA-256-PLUS\0\0"))
            .await
            .unwrap();
        server
    });
    let err = conn.startup().await.unwrap_err();
    assert!(
        matches!(&err, Error::Auth(m) if m.contains("SCRAM-SHA-256 is required")),
        "{err}"
    );

    for (kind, needle) in [(11u32, "SASL continue"), (12, "SASL final")] {
        let (mut conn, mut server) = pair();
        tokio::spawn(async move {
            read_startup(&mut server).await;
            server.write_all(&auth(kind, b"x")).await.unwrap();
            server
        });
        let err = conn.startup().await.unwrap_err();
        assert!(
            matches!(&err, Error::Protocol(m) if m.contains(needle)),
            "{err}"
        );
    }
}

#[quench_test::test]
async fn errors_and_surprises_during_startup() {
    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server
            .write_all(&error_response("28000", "no pg_hba.conf entry"))
            .await
            .unwrap();
        server
    });
    let err = conn.startup().await.unwrap_err();
    assert_eq!(err.code(), Some("28000"));

    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&frame(b'C', b"SELECT 1\0")).await.unwrap();
        server
    });
    assert!(matches!(
        conn.startup().await.unwrap_err(),
        Error::Protocol(_)
    ));

    // The connection closing during startup is an I/O error.
    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        drop(server);
    });
    assert!(matches!(conn.startup().await.unwrap_err(), Error::Io(_)));
    assert!(conn.is_closed());
}

#[quench_test::test]
async fn malformed_messages_close_the_connection() {
    // A length below the minimum.
    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&[b'R', 0, 0, 0, 2]).await.unwrap();
        server
    });
    assert!(matches!(
        conn.startup().await.unwrap_err(),
        Error::Protocol(_)
    ));
    assert!(conn.is_closed());

    // A length beyond the configured limit.
    let (client, mut server) = tokio::io::duplex(1 << 16);
    let mut conn = Connection::from_stream(
        Box::new(client),
        Arc::new(Config {
            max_message_size: 1024,
            ..Config::default()
        }),
    );
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&[b'D', 0, 0, 0xff, 0xff]).await.unwrap();
        server
    });
    assert!(
        matches!(conn.startup().await.unwrap_err(), Error::Protocol(m) if m.contains("out of range"))
    );

    // A message whose body is shorter than its fields claim.
    let (mut conn, mut server) = pair();
    tokio::spawn(async move {
        read_startup(&mut server).await;
        server.write_all(&frame(b'K', &[0, 0])).await.unwrap();
        server
    });
    assert!(matches!(
        conn.startup().await.unwrap_err(),
        Error::Protocol(_)
    ));
}

#[quench_test::test]
async fn a_data_row_without_a_description_is_a_protocol_error() {
    let (mut conn, mut server) = session().await;
    tokio::spawn(async move {
        read_tagged(&mut server).await;
        server
            .write_all(&frame(b'D', &[0, 1, 0, 0, 0, 1, b'x']))
            .await
            .unwrap();
        server
    });
    let err = conn.simple_query("SELECT 1").await.unwrap_err();
    assert!(matches!(err, Error::Protocol(_)), "{err}");
    assert!(conn.is_closed() || conn.simple_query("SELECT 1").await.is_err());
}

#[quench_test::test]
async fn invalid_utf8_in_a_value_is_a_protocol_error() {
    let (mut conn, mut server) = session().await;
    tokio::spawn(async move {
        read_tagged(&mut server).await;
        let description = [
            &[0u8, 1][..],
            b"c\0",
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0,
            ],
        ]
        .concat();
        server
            .write_all(
                &[
                    frame(b'T', &description),
                    frame(b'D', &[0, 1, 0, 0, 0, 2, 0xff, 0xfe]),
                    frame(b'C', b"SELECT 1\0"),
                    ready(),
                ]
                .concat(),
            )
            .await
            .unwrap();
        server
    });
    let err = conn.simple_query("SELECT x").await.unwrap_err();
    assert!(
        matches!(&err, Error::Protocol(m) if m.contains("UTF-8")),
        "{err}"
    );
}

#[quench_test::test]
async fn a_server_that_hangs_up_mid_query_closes_the_connection() {
    let (mut conn, mut server) = session().await;
    tokio::spawn(async move {
        read_tagged(&mut server).await;
        drop(server);
    });
    let err = conn.query("SELECT 1", &[]).await.unwrap_err();
    assert!(matches!(err, Error::Io(_)), "{err}");
    assert!(conn.is_closed());
    assert!(matches!(
        conn.query("SELECT 1", &[]).await,
        Err(Error::Closed)
    ));
    assert!(conn.looks_dead());
}

#[quench_test::test]
async fn extended_protocol_results_and_unknown_messages_are_handled() {
    let (mut conn, mut server) = session().await;
    let task = tokio::spawn(async move {
        let mut request = Vec::new();
        // Parse, Bind, Describe, Execute, Sync arrive as five messages.
        for _ in 0..5 {
            let (tag, body) = read_tagged(&mut server).await;
            request.push((tag, body));
        }
        let description = [
            &[0u8, 1][..],
            b"n\0",
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 0, 4, 0xff, 0xff, 0xff, 0xff, 0, 0,
            ],
        ]
        .concat();
        server
            .write_all(
                &[
                    frame(b'1', b""),
                    frame(b'2', b""),
                    frame(b'T', &description),
                    frame(b'D', &[0, 1, 0, 0, 0, 1, b'5']),
                    frame(b'D', &[0, 1, 0xff, 0xff, 0xff, 0xff]),
                    frame(b'?', b"mystery"),
                    frame(b'C', b"SELECT 2\0"),
                    ready(),
                ]
                .concat(),
            )
            .await
            .unwrap();
        request
    });
    let rows = conn
        .query(
            "SELECT n FROM t WHERE a = $1 AND b = $2",
            &[&"x", &None::<i32>],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<i32, _>("n").unwrap(), 5);
    assert_eq!(rows[1].get::<Option<i32>, _>("n").unwrap(), None);
    let request = task.await.unwrap();
    assert_eq!(
        request.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        b"PBDES"
    );
    let bind = &request[1].1;
    assert!(
        bind.windows(1).any(|w| w == b"x"),
        "the text parameter is in the Bind message"
    );
}

#[quench_test::test]
async fn too_many_parameters_are_refused_up_front() {
    let (mut conn, _server) = session().await;
    let values: Vec<i32> = vec![0; 40_000];
    let refs: Vec<&dyn quench_pg::ToSql> =
        values.iter().map(|v| v as &dyn quench_pg::ToSql).collect();
    let err = conn.query("SELECT 1", &refs).await.unwrap_err();
    assert!(matches!(err, Error::Config(_)), "{err}");
    assert!(!conn.is_closed());
}

#[quench_test::test]
async fn an_error_in_the_middle_of_a_result_is_reported_after_the_sync() {
    let (mut conn, mut server) = session().await;
    tokio::spawn(async move {
        for _ in 0..5 {
            read_tagged(&mut server).await;
        }
        server
            .write_all(
                &[
                    frame(b'1', b""),
                    error_response("22012", "division by zero"),
                    error_response("XX000", "second error"),
                    ready(),
                ]
                .concat(),
            )
            .await
            .unwrap();
        for _ in 0..5 {
            read_tagged(&mut server).await;
        }
        server
            .write_all(
                &[
                    frame(b'1', b""),
                    frame(b'2', b""),
                    frame(b'n', b""),
                    frame(b'C', b"INSERT 0 4\0"),
                    ready(),
                ]
                .concat(),
            )
            .await
            .unwrap();
        server
    });
    let err = conn.query("SELECT 1/0", &[]).await.unwrap_err();
    assert_eq!(
        err.code(),
        Some("22012"),
        "the first error is the one reported"
    );
    assert!(
        !conn.is_closed(),
        "the conversation completed, so the connection is fine"
    );
    assert_eq!(
        conn.execute("INSERT INTO t VALUES (1), (2), (3), (4)", &[])
            .await
            .unwrap(),
        4
    );
}

// ---- TLS negotiation over TCP ----------------------------------------------------------------

async fn tcp_server<F, Fut>(handler: F) -> u16
where
    F: FnOnce(tokio::net::TcpStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        handler(stream).await;
    });
    port
}

fn tcp_config(port: u16, mode: SslMode) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port,
        user: "u".into(),
        password: Some("pw".into()),
        ssl_mode: mode,
        ..Config::default()
    }
}

#[cfg(feature = "tls")]
async fn answer_ssl_request(stream: &mut tokio::net::TcpStream, answer: u8) {
    let mut request = [0u8; 8];
    stream.read_exact(&mut request).await.unwrap();
    assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
    stream.write_all(&[answer]).await.unwrap();
}

#[cfg(feature = "tls")]
#[quench_test::test]
async fn a_server_without_tls_is_fine_for_prefer_and_fatal_for_require() {
    let port = tcp_server(|mut stream| async move {
        answer_ssl_request(&mut stream, b'N').await;
        read_startup(&mut stream).await;
        stream
            .write_all(&[auth(0, &[]), ready()].concat())
            .await
            .unwrap();
        let _ = stream.read(&mut [0u8; 8]).await;
    })
    .await;
    Connection::connect(&tcp_config(port, SslMode::Prefer))
        .await
        .unwrap();

    let port = tcp_server(|mut stream| async move {
        answer_ssl_request(&mut stream, b'N').await;
    })
    .await;
    let err = Connection::connect(&tcp_config(port, SslMode::Require))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, Error::Tls(m) if m.contains("does not support TLS")),
        "{err}"
    );

    let port = tcp_server(|mut stream| async move {
        answer_ssl_request(&mut stream, b'X').await;
    })
    .await;
    assert!(matches!(
        Connection::connect(&tcp_config(port, SslMode::Prefer))
            .await
            .unwrap_err(),
        Error::Protocol(_)
    ));
}

#[cfg(feature = "tls")]
fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quench-tls/tests/fixtures")
        .join(name)
}

#[cfg(feature = "tls")]
async fn tls_server(port_tx: tokio::sync::oneshot::Sender<u16>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    port_tx.send(listener.local_addr().unwrap().port()).unwrap();
    let acceptor = quench_tls::TlsAcceptor::from(
        quench_tls::load_server_config(fixture("server.pem"), fixture("server.key")).unwrap(),
    );
    let (mut stream, _) = listener.accept().await.unwrap();
    answer_ssl_request(&mut stream, b'S').await;
    let Ok(mut tls) = acceptor.accept(stream).await else {
        return;
    };
    read_startup(&mut tls).await;
    tls.write_all(&[auth(0, &[]), ready()].concat())
        .await
        .unwrap();
    let (tag, _) = read_tagged(&mut tls).await;
    assert_eq!(tag, b'Q');
    let description = [
        &[0u8, 1][..],
        b"s\0",
        &[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0, 0,
        ],
    ]
    .concat();
    tls.write_all(
        &[
            frame(b'T', &description),
            frame(
                b'D',
                &[0, 1, 0, 0, 0, 7, b'e', b'n', b'c', b'r', b'y', b'p', b't'],
            ),
            frame(b'C', b"SELECT 1\0"),
            ready(),
        ]
        .concat(),
    )
    .await
    .unwrap();
    let _ = read_tagged(&mut tls).await;
}

#[cfg(feature = "tls")]
#[quench_test::test]
async fn tls_is_negotiated_and_verified_when_asked() {
    // `require` accepts any certificate.
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(tls_server(tx));
    let port = rx.await.unwrap();
    let mut conn = Connection::connect(&tcp_config(port, SslMode::Require))
        .await
        .unwrap();
    let results = conn.simple_query("SELECT 'encrypt'").await.unwrap();
    assert_eq!(
        results[0].rows[0].get::<String, _>(&0usize).unwrap(),
        "encrypt"
    );
    conn.close().await;

    // `verify-full` trusts only what it is told to; with the fixture CA the handshake succeeds.
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(tls_server(tx));
    let port = rx.await.unwrap();
    let mut config = tcp_config(port, SslMode::VerifyFull);
    config.ssl_root_cert_pem = vec![std::fs::read(fixture("ca.pem")).unwrap()];
    let mut conn = Connection::connect(&config).await.unwrap();
    assert_eq!(
        conn.simple_query("SELECT 'encrypt'").await.unwrap().len(),
        1
    );

    // Without the CA the certificate is untrusted.
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(tls_server(tx));
    let port = rx.await.unwrap();
    let err = Connection::connect(&tcp_config(port, SslMode::VerifyFull))
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Tls(_)), "{err}");
}

#[cfg(not(feature = "tls"))]
#[quench_test::test]
async fn without_the_tls_feature_prefer_stays_plain_and_require_is_refused() {
    let port = tcp_server(|mut stream| async move {
        // No SSLRequest: the startup message comes first.
        read_startup(&mut stream).await;
        stream
            .write_all(&[auth(0, &[]), ready()].concat())
            .await
            .unwrap();
        let _ = stream.read(&mut [0u8; 8]).await;
    })
    .await;
    Connection::connect(&tcp_config(port, SslMode::Prefer))
        .await
        .unwrap();

    for mode in [SslMode::Require, SslMode::VerifyFull] {
        let port = tcp_server(|_stream| async move {}).await;
        let err = Connection::connect(&tcp_config(port, mode))
            .await
            .unwrap_err();
        assert!(
            matches!(&err, Error::Tls(m) if m.contains("`tls` feature")),
            "{err}"
        );
    }
}

#[quench_test::test]
async fn connecting_can_time_out() {
    let port = tcp_server(|mut stream| async move {
        // Accept and say nothing.
        let _ = stream.read(&mut [0u8; 64]).await;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    })
    .await;
    let config = Config {
        connect_timeout: std::time::Duration::from_millis(150),
        ssl_mode: SslMode::Disable,
        ..tcp_config(port, SslMode::Disable)
    };
    let err = Connection::connect(&config).await.unwrap_err();
    assert!(matches!(err, Error::Timeout(_)), "{err}");
}

#[cfg(unix)]
#[quench_test::test]
async fn unix_sockets_are_used_for_slash_hosts() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let path = dir.path().join(".s.PGSQL.5432");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_startup(&mut stream).await;
        stream
            .write_all(&[auth(0, &[]), ready()].concat())
            .await
            .unwrap();
        let _ = stream.read(&mut [0u8; 8]).await;
    });
    let config = Config {
        host: dir.path().to_str().unwrap().to_string(),
        port: 5432,
        user: "u".into(),
        ..Config::default()
    };
    let conn = Connection::connect(&config).await.unwrap();
    assert!(!conn.is_closed());
    let missing = Config {
        host: "/no/such/dir".into(),
        ..config
    };
    assert!(matches!(
        Connection::connect(&missing).await.unwrap_err(),
        Error::Io(_)
    ));
}

#[quench_test::test]
async fn looks_dead_notices_a_peer_that_vanished() {
    let (mut conn, server) = session().await;
    assert!(!conn.looks_dead());
    drop(server);
    assert!(conn.looks_dead());
}

quench_test::main!();
