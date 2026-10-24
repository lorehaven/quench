//! The HTTP/1.1 client driver, against scripted servers and against the server driver.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::bytes::Bytes;
use quench_core::stream::StreamExt;
use quench_proto::h1::client::{ClientConfig, ClientConn, send_request};
use quench_proto::h1::server::{ServerConfig, Shutdown, serve_connection};
use quench_proto::{Body, Method, Request, Response, StatusCode, Uri};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn get(url: &str) -> Request {
    Request::new(Method::GET, Uri::parse(url).unwrap())
}

/// Spawns a scripted peer: reads one request head, replies with `reply`.
fn scripted(reply: &'static [u8]) -> (tokio::io::DuplexStream, tokio::task::JoinHandle<Vec<u8>>) {
    let (client, mut server) = tokio::io::duplex(1 << 16);
    let task = tokio::spawn(async move {
        let mut seen = Vec::new();
        let mut buf = [0u8; 4096];
        while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = server.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            seen.extend_from_slice(&buf[..n]);
        }
        server.write_all(reply).await.unwrap();
        server.shutdown().await.unwrap();
        seen
    });
    (client, task)
}

#[quench_test::test]
async fn sends_a_request_head_with_host_and_reads_a_sized_body() {
    let (io, peer) = scripted(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-A: b\r\n\r\nhello");
    let exchange = send_request(
        ClientConn::new(io),
        get("http://example.com:8080/a/b?q=1").header("accept", "*/*"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.status, StatusCode::OK);
    assert_eq!(exchange.response.headers.get("x-a").unwrap(), "b");
    assert_eq!(exchange.response.body.exact_len(), Some(5));
    let body = exchange.response.body.collect_limited(100).await.unwrap();
    assert_eq!(body, "hello");
    let seen = String::from_utf8(peer.await.unwrap()).unwrap();
    assert!(seen.starts_with("GET /a/b?q=1 HTTP/1.1\r\n"), "{seen}");
    assert!(
        seen.contains("host: example.com:8080\r\n") && seen.contains("accept: */*\r\n"),
        "{seen}"
    );
    assert!(!seen.contains("content-length"), "{seen}");
}

#[quench_test::test]
async fn userinfo_is_not_sent_in_the_host_header() {
    let (io, peer) = scripted(b"HTTP/1.1 204 No Content\r\n\r\n");
    let _ = send_request(
        ClientConn::new(io),
        get("http://user:pw@h.example/x"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    let seen = String::from_utf8(peer.await.unwrap()).unwrap();
    assert!(
        seen.contains("host: h.example\r\n") && !seen.contains("user:pw"),
        "{seen}"
    );
}

#[quench_test::test]
async fn posts_a_sized_body_with_content_length() {
    let (io, peer) = scripted(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n");
    let request = Request::new(Method::POST, Uri::parse("http://h/x").unwrap()).body("payload");
    let exchange = send_request(ClientConn::new(io), request, &ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(exchange.response.status, StatusCode::CREATED);
    let seen = String::from_utf8(peer.await.unwrap()).unwrap();
    assert!(
        seen.contains("content-length: 7\r\n") && seen.ends_with("payload"),
        "{seen}"
    );
}

#[quench_test::test]
async fn an_empty_post_still_declares_a_zero_length() {
    let (io, peer) = scripted(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let _ = send_request(
        ClientConn::new(io),
        Request::new(Method::POST, Uri::parse("http://h/x").unwrap()),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    let seen = String::from_utf8(peer.await.unwrap()).unwrap();
    assert!(seen.contains("content-length: 0\r\n"), "{seen}");
}

#[quench_test::test]
async fn streams_unsized_request_bodies_chunked() {
    let (io, peer) = scripted(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let items: Vec<Result<Bytes, std::io::Error>> = vec![
        Ok(Bytes::from_static(b"ab")),
        Ok(Bytes::from_static(b"cde")),
    ];
    let request = Request::new(Method::PUT, Uri::parse("http://h/x").unwrap())
        .body(Body::from_stream(quench_core::stream::iter(items)));
    let _ = send_request(ClientConn::new(io), request, &ClientConfig::default())
        .await
        .unwrap();
    let seen = String::from_utf8(peer.await.unwrap()).unwrap();
    assert!(
        seen.contains("transfer-encoding: chunked\r\n")
            && seen.ends_with("2\r\nab\r\n3\r\ncde\r\n0\r\n\r\n"),
        "{seen}"
    );
}

#[quench_test::test]
async fn declared_request_length_must_match_the_body() {
    let (io, _peer) = scripted(b"HTTP/1.1 200 OK\r\n\r\n");
    let request = Request::new(Method::POST, Uri::parse("http://h/x").unwrap())
        .header("content-length", "2")
        .body("abcdef");
    assert!(matches!(
        send_request(ClientConn::new(io), request, &ClientConfig::default()).await,
        Err(quench_proto::Error::BodyLength)
    ));
    let (io, _peer) = scripted(b"HTTP/1.1 200 OK\r\n\r\n");
    let request = Request::new(Method::POST, Uri::parse("http://h/x").unwrap())
        .header("content-length", "9")
        .body("abc");
    assert!(matches!(
        send_request(ClientConn::new(io), request, &ClientConfig::default()).await,
        Err(quench_proto::Error::BodyLength)
    ));
}

#[quench_test::test]
async fn a_failing_request_body_stream_aborts_the_send() {
    let (io, _peer) = scripted(b"HTTP/1.1 200 OK\r\n\r\n");
    let items: Vec<Result<Bytes, std::io::Error>> = vec![Err(std::io::Error::other("nope"))];
    let request = Request::new(Method::POST, Uri::parse("http://h/x").unwrap())
        .body(Body::from_stream(quench_core::stream::iter(items)));
    assert!(matches!(
        send_request(ClientConn::new(io), request, &ClientConfig::default()).await,
        Err(quench_proto::Error::Body(_))
    ));
}

#[quench_test::test]
async fn skips_interim_responses() {
    let (io, _peer) =
        scripted(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
    let exchange = send_request(
        ClientConn::new(io),
        get("http://h/"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.status, StatusCode::OK);
}

#[quench_test::test]
async fn reads_chunked_and_close_delimited_bodies() {
    let (io, _peer) = scripted(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n",
    );
    let exchange = send_request(
        ClientConn::new(io),
        get("http://h/"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.body.exact_len(), None);
    assert_eq!(
        exchange.response.body.collect_limited(100).await.unwrap(),
        "abcde"
    );

    let (io, _peer) = scripted(b"HTTP/1.0 200 OK\r\n\r\nuntil close");
    let exchange = send_request(
        ClientConn::new(io),
        get("http://h/"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        exchange.response.body.collect_limited(100).await.unwrap(),
        "until close"
    );
    assert!(
        exchange.reuse.await.is_err(),
        "a close-delimited body cannot be reused"
    );
}

#[quench_test::test]
async fn head_responses_have_no_body_even_with_a_length() {
    let (io, _peer) = scripted(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n");
    let request = Request::new(Method::HEAD, Uri::parse("http://h/").unwrap());
    let exchange = send_request(ClientConn::new(io), request, &ClientConfig::default())
        .await
        .unwrap();
    assert!(exchange.response.body.is_empty_body());
    assert!(exchange.reuse.await.is_ok());
}

#[quench_test::test]
async fn bad_responses_are_errors() {
    for reply in [
        &b"garbage\r\n\r\n"[..],
        b"HTTP/1.1 200 OK\r\nContent-Length: x\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\n\r\n",
    ] {
        let (io, _peer) = scripted(Box::leak(reply.to_vec().into_boxed_slice()));
        assert!(
            send_request(
                ClientConn::new(io),
                get("http://h/"),
                &ClientConfig::default()
            )
            .await
            .is_err()
        );
    }
    // Closed before any head.
    let (io, _peer) = scripted(b"");
    assert!(matches!(
        send_request(
            ClientConn::new(io),
            get("http://h/"),
            &ClientConfig::default()
        )
        .await,
        Err(quench_proto::Error::UnexpectedEof)
    ));
    // Head larger than the limit.
    let config = ClientConfig {
        limits: quench_proto::h1::Limits {
            max_head_bytes: 32,
            ..Default::default()
        },
        ..ClientConfig::default()
    };
    let (io, _peer) = scripted(
        b"HTTP/1.1 200 OK\r\nX-Long: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n\r\n",
    );
    assert!(
        send_request(ClientConn::new(io), get("http://h/"), &config)
            .await
            .is_err()
    );
}

#[quench_test::test]
async fn a_truncated_body_is_reported_by_the_stream() {
    let (io, _peer) = scripted(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc");
    let exchange = send_request(
        ClientConn::new(io),
        get("http://h/"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert!(exchange.response.body.collect_limited(100).await.is_err());
    assert!(exchange.reuse.await.is_err());
}

#[quench_test::test]
async fn a_silent_server_trips_the_head_timeout() {
    let (io, _server) = tokio::io::duplex(1024);
    let config = ClientConfig {
        response_head_timeout: Duration::from_millis(100),
        ..ClientConfig::default()
    };
    assert!(matches!(
        send_request(ClientConn::new(io), get("http://h/"), &config).await,
        Err(quench_proto::Error::Timeout(_))
    ));
}

#[quench_test::test]
async fn connections_are_reused_through_the_server_driver() {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let service = |req: Request| async move {
        let mut resp =
            Response::new(StatusCode::OK).body(format!("you asked for {}", req.uri.path()));
        resp.headers
            .insert("x-seen", quench_proto::HeaderValue::from_static("1"));
        resp
    };
    let server = tokio::spawn(async move {
        serve_connection(
            server_io,
            &service,
            &ServerConfig::default(),
            Shutdown::never(),
        )
        .await
    });

    let mut conn = ClientConn::new(client_io);
    for path in ["/one", "/two", "/three"] {
        let exchange = send_request(
            conn,
            get(&format!("http://h{path}")),
            &ClientConfig::default(),
        )
        .await
        .unwrap();
        let body = exchange.response.body.collect_limited(100).await.unwrap();
        assert_eq!(body, format!("you asked for {path}").as_str());
        conn = exchange.reuse.await.expect("the connection is reusable");
    }
    drop(conn);
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn bodies_can_be_streamed_item_by_item() {
    let (io, _peer) = scripted(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\na\r\n1\r\nb\r\n0\r\n\r\n",
    );
    let exchange = send_request(
        ClientConn::new(io),
        get("http://h/"),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    let mut body = exchange.response.body;
    let mut seen = Vec::new();
    while let Some(chunk) = body.next().await {
        seen.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(seen, b"ab");
}

#[quench_test::test]
async fn idle_connections_notice_when_the_peer_closed() {
    let (io, mut server) = tokio::io::duplex(1024);
    let mut conn = ClientConn::new(io);
    assert!(conn.is_reusable(), "a quiet connection is reusable");
    server.write_all(b"unsolicited").await.unwrap();
    assert!(!conn.is_reusable(), "stray bytes make it unusable");

    let (io, server) = tokio::io::duplex(1024);
    let mut conn = ClientConn::new(io);
    drop(server);
    assert!(!conn.is_reusable(), "a closed peer makes it unusable");
}

quench_test::main!();
