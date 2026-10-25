//! The framework above the parser, attacked over a real socket: routing and path-parameter decoding,
//! body limits, panic isolation, and header injection through `Response`.
//!
//! Where a behaviour is a deliberate contract that handlers must know about - a `%2f` in a path
//! segment reaches the handler decoded - the test pins it, so a change is a conscious decision and
//! never an accident.

#![allow(clippy::items_after_statements)]
#![allow(clippy::unused_async)]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_http::di::ContainerBuilder;
use quench_http::error::HttpError;
use quench_http::extract::{Json, Path, Query};
use quench_http::prelude::{get, post};
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_proto::StatusCode;
use quench_serde::Deserialize;
use std::fmt::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[get("/hello")]
async fn hello() -> &'static str {
    "hi"
}

#[get("/item/{name}")]
async fn item(Path(name): Path<String>) -> String {
    format!("name={name:?}")
}

#[get("/files/{path:.+}")]
async fn files(Path(path): Path<String>) -> String {
    format!("path={path:?}")
}

#[derive(Deserialize)]
struct Echo {
    v: String,
}

#[get("/reflect")]
async fn reflect(Query(q): Query<Echo>) -> Response {
    Response::text(StatusCode::OK, "ok").header("x-reflected", q.v)
}

#[derive(Deserialize)]
struct Small {
    ok: bool,
}

#[post("/json")]
async fn json(Json(body): Json<Small>) -> Result<String, HttpError> {
    Ok(format!("ok={}", body.ok))
}

#[get("/panic")]
async fn boom() -> &'static str {
    panic!("handler bug, deliberately triggered by the security tests");
}

async fn start() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app = Arc::new(quench_http::route::discover_routes());
    tokio::spawn(serve_http_on(
        app,
        container,
        listener,
        std::future::pending::<()>(),
    ));
    port
}

/// Writes `wire`, returns everything read until the server closes or `wait` passes.
async fn raw(port: u16, wire: &[u8], wait: Duration) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let _ = stream.write_all(wire).await;
    let mut out = Vec::new();
    let _ = tokio::time::timeout(wait, stream.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).into_owned()
}

async fn get_path(port: u16, path: &str) -> String {
    let wire = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    raw(port, wire.as_bytes(), Duration::from_secs(3)).await
}

fn status(reply: &str) -> u16 {
    reply.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0)
}

#[quench_test::test]
async fn baseline_route_works() {
    let port = start().await;
    let reply = get_path(port, "/hello").await;
    assert_eq!(status(&reply), 200, "{reply}");
}

#[quench_test::test]
async fn a_single_segment_parameter_never_spans_slashes() {
    let port = start().await;
    for path in ["/item/a/b", "/item//x", "/item/", "//item/x", "/item/x/"] {
        let reply = get_path(port, path).await;
        assert_eq!(status(&reply), 404, "{path} -> {reply}");
    }
}

#[quench_test::test]
async fn encoded_separators_reach_the_handler_decoded_exactly_once() {
    // The contract: routing sees the raw path, handlers see percent-decoded values. A handler that
    // uses a decoded name as a filesystem path must therefore validate it (warehouse does).
    let port = start().await;
    let reply = get_path(port, "/item/..%2f..%2fetc%2fpasswd").await;
    assert!(reply.contains(r#"name="../../etc/passwd""#), "{reply}");

    // Decoded once, not twice: %252e is the literal text %2e, never `.`.
    let reply = get_path(port, "/item/%252e%252e").await;
    assert!(reply.contains(r#"name="%2e%2e""#), "{reply}");
}

#[quench_test::test]
async fn hostile_percent_encodings_are_handled_without_a_server_error() {
    let port = start().await;
    for path in [
        "/item/%ff",
        "/item/%c0%af",
        "/item/%zz",
        "/item/%",
        "/item/%4",
        "/item/%00",
        "/item/a%00b",
    ] {
        let reply = get_path(port, path).await;
        let code = status(&reply);
        assert!(
            matches!(code, 200 | 400 | 404),
            "{path} -> {code} (a 5xx or a dropped connection means unhandled input)\n{reply}"
        );
        if code == 200 && path.contains("%00") {
            // A NUL must arrive intact, never truncate the value C-string style.
            assert!(
                reply.contains(r"name=\0")
                    || reply.contains(r#"name="\0""#)
                    || reply.contains(r#"name="a\0b""#),
                "NUL handling is lossy: {reply}"
            );
        }
        if path == "/item/%ff" {
            assert_eq!(
                code, 400,
                "invalid UTF-8 in a path parameter must be rejected: {reply}"
            );
        }
    }
}

#[quench_test::test]
async fn wildcard_parameters_capture_dot_segments_verbatim() {
    let port = start().await;
    let reply = get_path(port, "/files/../../etc/passwd").await;
    // The framework must not silently resolve `..`; whatever it does, the handler is told exactly
    // what was asked for, or the request never reaches it.
    assert!(
        status(&reply) == 404 || reply.contains(r#"path="../../etc/passwd""#),
        "dot segments were silently rewritten: {reply}"
    );
}

#[quench_test::test]
async fn method_override_headers_and_unknown_methods_are_not_honoured() {
    let port = start().await;
    for header in [
        "X-HTTP-Method-Override: GET",
        "X-Method-Override: GET",
        "X-HTTP-Method: GET",
    ] {
        let wire = format!(
            "POST /hello HTTP/1.1\r\nHost: x\r\n{header}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let reply = raw(port, wire.as_bytes(), Duration::from_secs(3)).await;
        assert_eq!(status(&reply), 405, "{header} changed the method: {reply}");
    }
    for method in ["FOO", "TRACE", "CONNECT", "PROPFIND"] {
        let wire = format!("{method} /hello HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
        let reply = raw(port, wire.as_bytes(), Duration::from_secs(3)).await;
        assert!(
            !matches!(status(&reply), 200..=299),
            "{method} succeeded on a GET-only route: {reply}"
        );
    }
}

#[quench_test::test]
async fn an_oversized_json_body_is_refused_without_being_buffered() {
    let port = start().await;
    let started = Instant::now();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    // Declare 100 MiB and stream 4 MiB of it: well over the 2 MiB default, far short of the claim.
    let head = "POST /json HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 104857600\r\nConnection: close\r\n\r\n";
    let _ = stream.write_all(head.as_bytes()).await;
    let chunk = vec![b' '; 64 * 1024];
    for _ in 0..64 {
        if stream.write_all(&chunk).await.is_err() {
            break;
        }
    }
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut out)).await;
    let reply = String::from_utf8_lossy(&out);
    assert!(
        !matches!(status(&reply), 200..=299),
        "oversized body accepted: {reply}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "took {:?}",
        started.elapsed()
    );
}

#[quench_test::test]
async fn a_panicking_handler_does_not_take_the_server_down() {
    let port = start().await;
    let reply = get_path(port, "/panic").await;
    assert!(
        reply.is_empty() || status(&reply) == 500,
        "a panic must end as a 500 or a closed connection, not a success: {reply}"
    );
    // Many at once, then prove the listener and the worker pool are both still healthy.
    let storm: Vec<_> = (0..32)
        .map(|_| tokio::spawn(async move { get_path(port, "/panic").await }))
        .collect();
    for task in storm {
        let _ = task.await;
    }
    let reply = get_path(port, "/hello").await;
    assert_eq!(
        status(&reply),
        200,
        "server unhealthy after panics: {reply}"
    );
}

#[quench_test::test]
async fn crlf_from_user_input_cannot_inject_response_headers() {
    let port = start().await;
    // Control: the route works and does reflect a benign value, so the checks below can fail.
    let control = get_path(port, "/reflect?v=plain")
        .await
        .to_ascii_lowercase();
    assert!(
        status(&control) == 200 && control.contains("x-reflected: plain"),
        "{control}"
    );
    for payload in [
        "a%0d%0aSet-Cookie:%20evil=1",
        "a%0aSet-Cookie:%20evil=1",
        "a%0d%0a%0d%0a<script>alert(1)</script>",
        "%E5%98%8A%E5%98%8DSet-Cookie:%20evil=1",
    ] {
        let reply = get_path(port, &format!("/reflect?v={payload}"))
            .await
            .to_ascii_lowercase();
        assert!(
            !reply.contains("\r\nset-cookie: evil=1"),
            "{payload}: header injected\n{reply}"
        );
        assert!(
            !reply.contains("<script>"),
            "{payload}: response split into a body\n{reply}"
        );
    }
}

#[quench_test::test]
async fn pipelined_floods_get_a_response_each_in_order_without_cross_talk() {
    // Responses to pipelined requests must come back 1:1 and in order - a response queue that
    // misaligns hands user A's body to user B.
    let port = start().await;
    let mut wire = String::new();
    for i in 0..50 {
        let _ = write!(wire, "GET /item/n{i} HTTP/1.1\r\nHost: x\r\n\r\n");
    }
    wire.push_str("GET /hello HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    let reply = raw(port, wire.as_bytes(), Duration::from_secs(5)).await;
    assert_eq!(reply.matches("HTTP/1.1 200").count(), 51, "{reply}");
    let mut last = 0;
    for i in 0..50 {
        let at = reply
            .find(&format!("name=\"n{i}\""))
            .unwrap_or_else(|| panic!("response {i} missing"));
        assert!(at >= last, "response {i} out of order");
        last = at;
    }
}

quench_test::main!();
