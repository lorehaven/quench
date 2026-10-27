//! The HTTP/1.1 server driver, exercised over in-memory pipes with raw wire bytes.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::bytes::Bytes;
use quench_core::stream::StreamExt;
use quench_proto::h1::server::{ServerConfig, Shutdown, serve_connection, shutdown_channel};
use quench_proto::{Body, Request, Response, StatusCode};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn text(status: StatusCode, body: &'static str) -> Response {
    Response::new(status).body(body)
}

async fn echo(mut req: Request) -> Response {
    let body = std::mem::take(&mut req.body);
    let bytes = body.collect_limited(1 << 20).await.unwrap_or_default();
    let mut resp = Response::new(StatusCode::OK)
        .header("x-method", req.method.as_str())
        .header("x-path", req.uri.path());
    resp.body = Body::from_bytes(bytes);
    resp
}

/// Writes `wire`, half-closes, and returns everything the server sent back.
async fn run<F, Fut>(service: F, config: ServerConfig, wire: &[u8]) -> String
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    let (mut client, server) = tokio::io::duplex(1 << 20);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    client.write_all(wire).await.unwrap();
    client.shutdown().await.unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let _ = task.await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

fn quick() -> ServerConfig {
    ServerConfig {
        header_read_timeout: Duration::from_millis(300),
        idle_timeout: Duration::from_millis(300),
        write_timeout: Duration::from_millis(500),
        drain_timeout: Duration::from_millis(200),
        ..ServerConfig::default()
    }
}

#[quench_test::test]
async fn answers_a_simple_get_with_a_content_length() {
    let out = run(
        |_| async { text(StatusCode::OK, "hello") },
        quick(),
        b"GET /x HTTP/1.1\r\nHost: a\r\n\r\n",
    )
    .await;
    assert!(out.starts_with("HTTP/1.1 200 OK\r\n"), "{out}");
    assert!(out.contains("content-length: 5\r\n"), "{out}");
    assert!(out.contains("date: "), "{out}");
    assert!(out.ends_with("\r\n\r\nhello"), "{out}");
}

#[quench_test::test]
async fn keeps_the_connection_alive_for_pipelined_requests() {
    let wire = b"GET /one HTTP/1.1\r\nHost: a\r\n\r\n\r\nPOST /two HTTP/1.1\r\nHost: a\r\nContent-Length: 3\r\n\r\nabcGET /three HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n";
    let out = run(echo, quick(), wire).await;
    assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 3, "{out}");
    assert!(
        out.contains("x-path: /one")
            && out.contains("x-path: /two")
            && out.contains("x-path: /three"),
        "{out}"
    );
    assert!(out.contains("x-method: POST"), "{out}");
    assert!(out.contains("abc"), "{out}");
    assert!(out.contains("connection: close"), "{out}");
}

#[quench_test::test]
async fn decodes_a_chunked_request_body() {
    let wire = b"POST /up HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    let out = run(echo, quick(), wire).await;
    assert!(out.ends_with("\r\n\r\nhello world"), "{out}");
}

#[quench_test::test]
async fn streams_unsized_response_bodies_as_chunked() {
    let service = |_req: Request| async {
        let items: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"ab")),
            Ok(Bytes::new()),
            Ok(Bytes::from_static(b"cdef")),
        ];
        Response::new(StatusCode::OK).body(Body::from_stream(quench_core::stream::iter(items)))
    };
    let out = run(
        service,
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(out.contains("transfer-encoding: chunked\r\n"), "{out}");
    assert!(out.ends_with("2\r\nab\r\n4\r\ncdef\r\n0\r\n\r\n"), "{out}");
}

#[quench_test::test]
async fn http_10_clients_get_a_close_delimited_stream() {
    let service = |_req: Request| async {
        let items: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from_static(b"raw"))];
        Response::new(StatusCode::OK).body(Body::from_stream(quench_core::stream::iter(items)))
    };
    let out = run(service, quick(), b"GET / HTTP/1.0\r\n\r\n").await;
    assert!(out.contains("connection: close"), "{out}");
    assert!(!out.contains("transfer-encoding"), "{out}");
    assert!(out.ends_with("\r\n\r\nraw"), "{out}");
}

#[quench_test::test]
async fn http_10_keep_alive_is_acknowledged() {
    let wire = b"GET /a HTTP/1.0\r\nConnection: keep-alive\r\n\r\nGET /b HTTP/1.0\r\n\r\n";
    let out = run(|_| async { text(StatusCode::OK, "x") }, quick(), wire).await;
    assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 2, "{out}");
    assert!(out.contains("connection: keep-alive"), "{out}");
}

#[quench_test::test]
async fn head_requests_get_headers_but_no_body() {
    let out = run(
        |_| async { text(StatusCode::OK, "hello") },
        quick(),
        b"HEAD / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(out.contains("content-length: 5\r\n"), "{out}");
    assert!(out.ends_with("\r\n\r\n"), "{out}");
}

#[quench_test::test]
async fn bodyless_statuses_drop_their_framing_headers() {
    let service = |_req: Request| async {
        Response::new(StatusCode::NO_CONTENT).header("content-length", "9")
    };
    let out = run(
        service,
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        out.starts_with("HTTP/1.1 204 No Content\r\n") && !out.contains("content-length"),
        "{out}"
    );
}

#[quench_test::test]
async fn a_user_declared_content_length_is_honoured_and_checked() {
    let ok = |_req: Request| async {
        Response::new(StatusCode::OK)
            .header("content-length", "3")
            .body(Body::from_stream(quench_core::stream::iter(vec![Ok::<
                _,
                std::io::Error,
            >(
                Bytes::from_static(b"abc"),
            )])))
    };
    let out = run(
        ok,
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        out.contains("content-length: 3") && out.ends_with("abc"),
        "{out}"
    );

    // Too many bytes: the response is cut off and the connection dropped.
    let long = |_req: Request| async {
        Response::new(StatusCode::OK)
            .header("content-length", "2")
            .body("abcdef")
    };
    let out = run(long, quick(), b"GET / HTTP/1.1\r\nHost: a\r\n\r\n").await;
    assert!(out.is_empty() || !out.contains("abcdef"), "{out}");

    // Too few.
    let short = |_req: Request| async {
        Response::new(StatusCode::OK)
            .header("content-length", "10")
            .body("abc")
    };
    let out = run(short, quick(), b"GET / HTTP/1.1\r\nHost: a\r\n\r\n").await;
    assert!(!out.contains("connection: keep-alive"), "{out}");
}

#[quench_test::test]
async fn rejects_request_smuggling_attempts_and_closes() {
    for wire in [
        &b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"[..],
        b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\nContent-Length: 6\r\n\r\nhello!",
        b"POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: xchunked\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: a\r\nX : y\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: a\r\n folded: y\r\n\r\n",
        b"GET / HTTP/1.1\r\n\r\n",
        b"GET / HTTP/1.1\r\nHost: a\nX: y\r\n\r\n",
    ] {
        let out = run(echo, quick(), wire).await;
        assert!(out.starts_with("HTTP/1.1 400 Bad Request\r\n") || out.starts_with("HTTP/1.1 501"), "{}: {out}", String::from_utf8_lossy(wire));
        assert!(out.contains("connection: close"), "{out}");
    }
}

#[quench_test::test]
async fn maps_protocol_errors_to_their_statuses() {
    let out = run(echo, quick(), b"GET / HTTP/2.0\r\nHost: a\r\n\r\n").await;
    assert!(out.starts_with("HTTP/1.1 505 "), "{out}");
    let long_target = format!("GET /{} HTTP/1.1\r\nHost: a\r\n\r\n", "a".repeat(9000));
    let out = run(echo, quick(), long_target.as_bytes()).await;
    assert!(out.starts_with("HTTP/1.1 414 "), "{out}");
    let many = format!(
        "GET / HTTP/1.1\r\nHost: a\r\n{}\r\n",
        "X: y\r\n".repeat(150)
    );
    let out = run(echo, quick(), many.as_bytes()).await;
    assert!(out.starts_with("HTTP/1.1 431 "), "{out}");
    let huge = format!(
        "GET / HTTP/1.1\r\nHost: a\r\nX: {}\r\n\r\n",
        "y".repeat(70_000)
    );
    let out = run(echo, quick(), huge.as_bytes()).await;
    assert!(out.starts_with("HTTP/1.1 431 "), "{out}");
}

#[quench_test::test]
async fn a_truncated_head_is_an_error_without_a_response() {
    let (mut client, server) = tokio::io::duplex(1024);
    let task =
        tokio::spawn(
            async move { serve_connection(server, &echo, &quick(), Shutdown::never()).await },
        );
    client.write_all(b"GET / HTT").await.unwrap();
    client.shutdown().await.unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    assert!(out.is_empty());
    assert!(task.await.unwrap().is_err());
}

#[quench_test::test]
async fn a_stalled_head_times_out() {
    let (mut client, server) = tokio::io::duplex(1024);
    let task =
        tokio::spawn(
            async move { serve_connection(server, &echo, &quick(), Shutdown::never()).await },
        );
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n")
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(
        matches!(result, Err(quench_proto::Error::Timeout(_))),
        "{result:?}"
    );
}

#[quench_test::test]
async fn an_idle_connection_is_closed_quietly() {
    let (_client, server) = tokio::io::duplex(1024);
    let result = serve_connection(server, &echo, &quick(), Shutdown::never()).await;
    assert!(result.is_ok());
}

#[quench_test::test]
async fn shutdown_closes_idle_connections_and_marks_in_flight_responses() {
    let (handle, shutdown) = shutdown_channel();
    let (mut client, server) = tokio::io::duplex(4096);
    let config = ServerConfig {
        idle_timeout: Duration::from_secs(30),
        ..quick()
    };
    let task =
        tokio::spawn(async move { serve_connection(server, &echo, &config, shutdown).await });
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: a\r\n\r\n")
        .await
        .unwrap();
    let mut buf = vec![0u8; 512];
    let n = client.read(&mut buf).await.unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("200 OK"));
    handle.signal();
    // The idle connection notices and exits.
    assert!(task.await.unwrap().is_ok());
}

#[quench_test::test]
async fn shutdown_during_a_request_sends_connection_close() {
    let (handle, shutdown) = shutdown_channel();
    let handle = std::sync::Arc::new(handle);
    let in_handler = handle.clone();
    let service = move |_req: Request| {
        let handle = in_handler.clone();
        async move {
            handle.signal();
            text(StatusCode::OK, "bye")
        }
    };
    let (mut client, server) = tokio::io::duplex(4096);
    let task =
        tokio::spawn(async move { serve_connection(server, &service, &quick(), shutdown).await });
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: a\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(
        out.contains("connection: close") && out.ends_with("bye"),
        "{out}"
    );
    assert!(task.await.unwrap().is_ok());
}

#[quench_test::test]
async fn expect_continue_gets_an_interim_response() {
    let wire = b"POST / HTTP/1.1\r\nHost: a\r\nExpect: 100-continue\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi";
    let out = run(echo, quick(), wire).await;
    assert!(
        out.starts_with("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK"),
        "{out}"
    );
    assert!(out.ends_with("hi"), "{out}");
}

#[quench_test::test]
async fn unknown_expectations_are_refused() {
    let out = run(
        echo,
        quick(),
        b"POST / HTTP/1.1\r\nHost: a\r\nExpect: magic\r\nContent-Length: 2\r\n\r\nhi",
    )
    .await;
    assert!(out.starts_with("HTTP/1.1 417 "), "{out}");
}

#[quench_test::test]
async fn an_unread_request_body_is_drained_so_the_connection_survives() {
    let body = "x".repeat(200_000);
    let wire = format!(
        "POST /ignore HTTP/1.1\r\nHost: a\r\nContent-Length: {}\r\n\r\n{body}GET /next HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let out = run(
        |_| async { text(StatusCode::OK, "ignored") },
        quick(),
        wire.as_bytes(),
    )
    .await;
    assert_eq!(
        out.matches("HTTP/1.1 200 OK").count(),
        2,
        "{}",
        &out[..out.len().min(400)]
    );
}

#[quench_test::test]
async fn an_unread_body_past_the_drain_limit_closes_the_connection() {
    let config = ServerConfig {
        drain_limit: 1000,
        ..quick()
    };
    let body = "x".repeat(50_000);
    let wire = format!(
        "POST /big HTTP/1.1\r\nHost: a\r\nContent-Length: {}\r\n\r\n{body}GET /next HTTP/1.1\r\nHost: a\r\n\r\n",
        body.len()
    );
    let out = run(
        |_| async { text(StatusCode::PAYLOAD_TOO_LARGE, "no") },
        config,
        wire.as_bytes(),
    )
    .await;
    assert_eq!(out.matches("HTTP/1.1 ").count(), 1, "{out}");
    assert!(out.contains("connection: close"), "{out}");
}

#[quench_test::test]
async fn an_early_response_to_a_slow_upload_is_still_delivered() {
    let (mut client, server) = tokio::io::duplex(1 << 16);
    let config = ServerConfig {
        drain_timeout: Duration::from_millis(100),
        ..quick()
    };
    let task = tokio::spawn(async move {
        serve_connection(
            server,
            &|_req: Request| async { text(StatusCode::FORBIDDEN, "nope") },
            &config,
            Shutdown::never(),
        )
        .await
    });
    client
        .write_all(b"POST /x HTTP/1.1\r\nHost: a\r\nContent-Length: 100\r\n\r\npartial")
        .await
        .unwrap();
    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 403 Forbidden") && head.contains("connection: close"),
        "{head}"
    );
    drop(client);
    let _ = task.await.unwrap();
}

#[quench_test::test]
async fn a_truncated_request_body_surfaces_as_a_body_error() {
    let service = |mut req: Request| async move {
        let body = std::mem::take(&mut req.body);
        match body.collect_limited(1024).await {
            Ok(_) => text(StatusCode::OK, "complete"),
            Err(_) => text(StatusCode::BAD_REQUEST, "broken"),
        }
    };
    let (mut client, server) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &quick(), Shutdown::never()).await
    });
    client
        .write_all(b"POST / HTTP/1.1\r\nHost: a\r\nContent-Length: 50\r\n\r\nshort")
        .await
        .unwrap();
    client.shutdown().await.unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    assert!(String::from_utf8_lossy(&out).contains("400 Bad Request"));
    let _ = task.await.unwrap();
}

#[quench_test::test]
async fn bad_chunked_framing_in_the_body_is_reported_to_the_handler() {
    let service = |mut req: Request| async move {
        let body = std::mem::take(&mut req.body);
        match body.collect_limited(1024).await {
            Ok(_) => text(StatusCode::OK, "complete"),
            Err(_) => text(StatusCode::BAD_REQUEST, "broken"),
        }
    };
    let out = run(
        service,
        quick(),
        b"POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
    )
    .await;
    assert!(
        out.contains("400 Bad Request") && out.contains("broken"),
        "{out}"
    );
}

#[quench_test::test]
async fn upgrades_hand_over_the_raw_stream_with_buffered_bytes() {
    let service = |_req: Request| async {
        Response::new(StatusCode::SWITCHING_PROTOCOLS)
            .header("upgrade", "echo")
            .header("connection", "Upgrade")
            .on_upgrade(|mut io| async move {
                let mut buf = [0u8; 5];
                io.read_exact(&mut buf).await.unwrap();
                io.write_all(&buf).await.unwrap();
                io.shutdown().await.unwrap();
            })
    };
    let (mut client, server) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &quick(), Shutdown::never()).await
    });
    client
        .write_all(
            b"GET / HTTP/1.1\r\nHost: a\r\nConnection: Upgrade\r\nUpgrade: echo\r\n\r\nhello",
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(
        out.starts_with("HTTP/1.1 101 Switching Protocols\r\n") && out.ends_with("\r\n\r\nhello"),
        "{out}"
    );
    assert!(task.await.unwrap().is_ok());
}

#[quench_test::test]
async fn a_response_can_stream_the_request_body_back() {
    let service = |mut req: Request| async move {
        let body = std::mem::take(&mut req.body);
        Response::new(StatusCode::OK).body(body)
    };
    let wire = b"POST / HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n";
    let out = run(service, quick(), wire).await;
    assert!(
        out.contains("transfer-encoding: chunked") && out.contains("abc") && out.contains("def"),
        "{out}"
    );
}

#[quench_test::test]
async fn a_failing_response_stream_drops_the_connection() {
    let service = |_req: Request| async {
        let items: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"start")),
            Err(std::io::Error::other("producer died")),
        ];
        Response::new(StatusCode::OK).body(Body::from_stream(quench_core::stream::iter(items)))
    };
    let (mut client, server) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &quick(), Shutdown::never()).await
    });
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    client.read_to_end(&mut out).await.unwrap();
    assert!(!String::from_utf8_lossy(&out).ends_with("0\r\n\r\n"));
    assert!(matches!(
        task.await.unwrap(),
        Err(quench_proto::Error::Body(_))
    ));
}

#[quench_test::test]
async fn large_fixed_responses_are_written_whole() {
    let big = "z".repeat(100_000);
    let big2 = big.clone();
    let out = run(
        move |_| {
            let b = big2.clone();
            async move { Response::new(StatusCode::OK).body(b) }
        },
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(out.ends_with(&big), "len {}", out.len());
}

#[quench_test::test]
async fn a_slow_reader_trips_the_write_timeout() {
    // The pipe holds 64 bytes and nobody reads: the response cannot be written.
    let (mut client, server) = tokio::io::duplex(64);
    let config = ServerConfig {
        write_timeout: Duration::from_millis(100),
        ..quick()
    };
    let task = tokio::spawn(async move {
        serve_connection(
            server,
            &|_req: Request| async { text(StatusCode::OK, "x") },
            &config,
            Shutdown::never(),
        )
        .await
    });
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n")
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(
        matches!(result, Err(quench_proto::Error::Timeout(_))),
        "{result:?}"
    );
}

#[quench_test::test]
async fn streams_with_a_declared_length_use_content_length() {
    let service = |_req: Request| async {
        let items: Vec<Result<Bytes, std::io::Error>> =
            vec![Ok(Bytes::from_static(b"ab")), Ok(Bytes::from_static(b"cd"))];
        Response::new(StatusCode::OK)
            .body(Body::from_stream_sized(quench_core::stream::iter(items), 4))
    };
    let out = run(
        service,
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        out.contains("content-length: 4") && out.ends_with("abcd"),
        "{out}"
    );
    let mut body = Body::from_stream(quench_core::stream::iter(Vec::<
        Result<Bytes, std::io::Error>,
    >::new()));
    assert!(body.next().await.is_none());
}

quench_test::main!();

#[quench_test::test]
async fn a_streaming_response_sends_its_head_before_its_first_chunk_is_ready() {
    // A server-sent event stream may have nothing to say for a long time; the client still has
    // to learn that the response began (a browser's EventSource opens on the head).
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    let rx = std::sync::Mutex::new(Some(rx));
    let service = move |_req: Request| {
        let rx = rx.lock().unwrap().take().expect("one request");
        async move {
            let stream = quench_core::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            });
            Response::new(StatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(stream))
        }
    };
    let (mut client, server) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &quick(), Shutdown::never()).await
    });
    client
        .write_all(b"GET /events HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    // Nothing has been produced, yet the head arrives.
    let mut head = vec![0u8; 512];
    let n = tokio::time::timeout(Duration::from_millis(250), client.read(&mut head))
        .await
        .expect("the head must not wait for the first chunk")
        .unwrap();
    let head = String::from_utf8_lossy(&head[..n]).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 200 OK\r\n") && head.contains("transfer-encoding: chunked\r\n"),
        "{head}"
    );
    assert!(head.ends_with("\r\n\r\n"), "only the head so far: {head:?}");

    // Then the events follow as they happen, and the stream ends cleanly.
    tx.send(Ok(Bytes::from_static(b"data: hi\n\n")))
        .await
        .unwrap();
    drop(tx);
    let mut rest = Vec::new();
    client.read_to_end(&mut rest).await.unwrap();
    let rest = String::from_utf8_lossy(&rest);
    assert!(
        rest.contains("data: hi") && rest.ends_with("0\r\n\r\n"),
        "{rest:?}"
    );
    assert!(task.await.unwrap().is_ok());
}

#[quench_test::test]
async fn a_streaming_response_with_its_first_chunk_ready_is_still_one_write() {
    let service = |_req: Request| async {
        let items: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from_static(b"ready"))];
        Response::new(StatusCode::OK).body(Body::from_stream(quench_core::stream::iter(items)))
    };
    let out = run(
        service,
        quick(),
        b"GET / HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(out.ends_with("5\r\nready\r\n0\r\n\r\n"), "{out}");
}
