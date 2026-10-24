//! The mock server, driven by hand-written HTTP.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_serde::json;
use quench_test::mock::matchers::{
    any, basic_auth, body_bytes, body_json, body_json_string, body_string, body_string_contains,
    header, header_exists, header_regex, method, path, path_regex, query_param,
};
use quench_test::mock::{Mock, MockServer, Request, ResponseTemplate};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// Sends `raw` and returns everything the server writes before closing (the request asks it to).
fn send(server: &MockServer, raw: &str) -> String {
    let mut stream = TcpStream::connect(server.address()).unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

fn get(server: &MockServer, target: &str, extra: &str) -> String {
    send(
        server,
        &format!("GET {target} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra}\r\n"),
    )
}

fn post(server: &MockServer, target: &str, headers: &str, body: &str) -> String {
    send(
        server,
        &format!(
            "POST {target} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n{body}",
            body.len()
        ),
    )
}

fn body_of(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, b)| b)
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn answers_from_rules_and_404s_otherwise() {
    let server = MockServer::start().await;
    assert!(server.uri().starts_with("http://127.0.0.1:"));
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hi"))
        .mount(&server)
        .await;
    Mock::given(method("get"))
        .and(path("/json"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"a": 1})))
        .mount(&server)
        .await;
    server
        .register(
            Mock::given(path("/raw")).respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("<b/>", "text/xml")
                    .insert_header("X-One", "1")
                    .insert_header("x-one", "2")
                    .append_header("x-two", "a")
                    .append_header("x-two", "b"),
            ),
        )
        .await;
    Mock::given(path("/bytes"))
        .respond_with(ResponseTemplate::new(206).set_body_bytes(vec![0u8, 1, 2]))
        .mount(&server)
        .await;

    let r = get(&server, "/hello", "");
    assert!(
        r.starts_with("HTTP/1.1 200 OK\r\n")
            && r.contains("content-type: text/plain")
            && body_of(&r) == "hi",
        "{r}"
    );
    let r = get(&server, "/json", "");
    assert!(
        r.starts_with("HTTP/1.1 201 Created")
            && r.contains("application/json")
            && body_of(&r) == "{\"a\":1}",
        "{r}"
    );
    let r = get(&server, "/raw", "");
    assert!(
        r.contains("x-two: a")
            && r.contains("x-two: b")
            && r.contains("text/xml")
            && r.contains("x-one: 2")
            && !r.contains("X-One: 1"),
        "{r}"
    );
    assert!(get(&server, "/bytes", "").starts_with("HTTP/1.1 206 Partial Content"));
    let r = get(&server, "/missing", "");
    assert!(
        r.starts_with("HTTP/1.1 404 Not Found") && r.contains("content-length: 0"),
        "{r}"
    );
    assert!(post(&server, "/hello", "", "x").starts_with("HTTP/1.1 404"));
    server.verify().await;
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn matchers_look_at_the_whole_request() {
    let server = MockServer::start().await;
    let ok = || ResponseTemplate::new(200);
    Mock::given(path_regex("^/items/[0-9]+$"))
        .and(method("GET"))
        .respond_with(ok().set_body_string("regex"))
        .mount(&server)
        .await;
    Mock::given(header("x-token", "abc"))
        .and(path("/h"))
        .respond_with(ok().set_body_string("header"))
        .mount(&server)
        .await;
    Mock::given(header_regex("x-id", "^id-\\d+$"))
        .and(path("/hr"))
        .respond_with(ok().set_body_string("header regex"))
        .mount(&server)
        .await;
    Mock::given(header_exists("x-present"))
        .and(path("/he"))
        .respond_with(ok().set_body_string("exists"))
        .mount(&server)
        .await;
    Mock::given(query_param("q", "a b&c"))
        .and(path("/q"))
        .respond_with(ok().set_body_string("query"))
        .mount(&server)
        .await;
    Mock::given(body_string("exact"))
        .and(path("/bs"))
        .respond_with(ok().set_body_string("body string"))
        .mount(&server)
        .await;
    Mock::given(body_string_contains("needle"))
        .and(path("/bc"))
        .respond_with(ok().set_body_string("contains"))
        .mount(&server)
        .await;
    Mock::given(body_bytes(b"\x00\x01".to_vec()))
        .and(path("/bb"))
        .respond_with(ok().set_body_string("bytes"))
        .mount(&server)
        .await;
    Mock::given(body_json(json!({"k": [1, 2]})))
        .and(path("/bj"))
        .respond_with(ok().set_body_string("json"))
        .mount(&server)
        .await;
    Mock::given(body_json_string("{ \"k\": true }"))
        .and(path("/bjs"))
        .respond_with(ok().set_body_string("json string"))
        .mount(&server)
        .await;
    Mock::given(basic_auth("user", "pass"))
        .and(path("/auth"))
        .respond_with(ok().set_body_string("auth"))
        .mount(&server)
        .await;
    Mock::given(|r: &Request| r.path == "/closure")
        .respond_with(ok().set_body_string("closure"))
        .mount(&server)
        .await;

    assert_eq!(body_of(&get(&server, "/items/42", "")), "regex");
    assert!(get(&server, "/items/x", "").contains("404"));
    assert_eq!(body_of(&get(&server, "/h", "X-Token: abc\r\n")), "header");
    assert!(get(&server, "/h", "X-Token: nope\r\n").contains("404"));
    assert_eq!(
        body_of(&get(&server, "/hr", "X-Id: id-7\r\n")),
        "header regex"
    );
    assert_eq!(body_of(&get(&server, "/he", "X-Present: \r\n")), "exists");
    assert!(get(&server, "/he", "").contains("404"));
    assert_eq!(body_of(&get(&server, "/q?q=a%20b%26c", "")), "query");
    assert_eq!(body_of(&get(&server, "/q?x=1&q=a+b%26c", "")), "query");
    assert!(get(&server, "/q?q=other", "").contains("404"));
    assert_eq!(body_of(&post(&server, "/bs", "", "exact")), "body string");
    assert!(post(&server, "/bs", "", "exactly").contains("404"));
    assert_eq!(
        body_of(&post(&server, "/bc", "", "a needle here")),
        "contains"
    );
    assert_eq!(body_of(&post(&server, "/bb", "", "\u{0}\u{1}")), "bytes");
    assert_eq!(body_of(&post(&server, "/bj", "", "{\"k\": [1,2]}")), "json");
    assert!(post(&server, "/bj", "", "not json").contains("404"));
    assert_eq!(
        body_of(&post(&server, "/bjs", "", "{\"k\":true}")),
        "json string"
    );
    assert_eq!(
        body_of(&get(
            &server,
            "/auth",
            "Authorization: Basic dXNlcjpwYXNz\r\n"
        )),
        "auth"
    );
    assert!(get(&server, "/auth", "Authorization: Basic eDp5\r\n").contains("404"));
    assert_eq!(body_of(&get(&server, "/closure", "")), "closure");
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn priority_limits_and_requests() {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("fallback"))
        .with_priority(9)
        .mount(&server)
        .await;
    Mock::given(path("/p"))
        .respond_with(ResponseTemplate::new(200).set_body_string("first"))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(path("/p"))
        .respond_with(ResponseTemplate::new(200).set_body_string("second"))
        .mount(&server)
        .await;
    let seen: Vec<String> = (0..4)
        .map(|_| body_of(&get(&server, "/p", "")).to_string())
        .collect();
    assert_eq!(seen, ["first", "first", "second", "second"]);
    assert_eq!(body_of(&get(&server, "/other", "")), "fallback");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 5);
    let last = requests.last().unwrap();
    assert_eq!(
        (
            last.method.as_str(),
            last.path.as_str(),
            last.query.as_str()
        ),
        ("GET", "/other", "")
    );
    assert_eq!(last.header("HOST"), Some("x"));
    assert_eq!(last.header("nope"), None);
    post(
        &server,
        "/echo?a=1&b=%7B",
        "Content-Type: application/json\r\n",
        "{\"v\":1}",
    );
    let echoed = server.received_requests().await.unwrap().pop().unwrap();
    assert_eq!(echoed.query_param("b").as_deref(), Some("{"));
    assert_eq!(echoed.query_param("missing"), None);
    assert_eq!(echoed.body_string(), "{\"v\":1}");
    assert_eq!(echoed.body_json(), Some(json!({"v": 1})));
    server.reset().await;
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(get(&server, "/p", "").contains("404"));
    assert!(format!("{server:?}").starts_with("MockServer(127.0.0.1:"));
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn expectations_are_verified() {
    let server = MockServer::start().await;
    Mock::given(path("/once"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .named("once")
        .mount(&server)
        .await;
    Mock::given(path("/range"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..=2)
        .mount(&server)
        .await;
    Mock::given(path("/open"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1..)
        .mount(&server)
        .await;
    Mock::given(path("/half"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0..3)
        .mount(&server)
        .await;
    get(&server, "/once", "");
    get(&server, "/range", "");
    get(&server, "/open", "");
    server.verify().await;
    get(&server, "/range", "");
    server.verify().await;
    get(&server, "/range", "");
    get(&server, "/once", "");
    let failure = quench_core::future::catch_unwind(std::panic::AssertUnwindSafe(server.verify()))
        .await
        .unwrap_err();
    let text = failure
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        text.contains("`once` expected exactly 1 call(s) but got 2")
            && text.contains("(unnamed) expected between 1 and 2 call(s) but got 3"),
        "{text}"
    );
    // Dropping a verified server is quiet; a broken one is reported by `verify` only here.
    server.reset().await;
}

#[quench_test::test]
fn dropping_a_server_with_unmet_expectations_panics() {
    let result = std::panic::catch_unwind(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(path("/never"))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;
        });
    });
    assert!(result.is_err());
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 2)]
async fn keep_alive_chunked_bodies_and_delays() {
    let server = MockServer::start().await;
    Mock::given(path("/a"))
        .respond_with(ResponseTemplate::new(200).set_body_string("one"))
        .mount(&server)
        .await;
    Mock::given(path("/b"))
        .respond_with(ResponseTemplate::new(200).set_body_string("two"))
        .mount(&server)
        .await;
    Mock::given(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)))
        .mount(&server)
        .await;
    Mock::given(body_string("chunked body"))
        .and(path("/c"))
        .respond_with(ResponseTemplate::new(200).set_body_string("chunks"))
        .mount(&server)
        .await;

    // Two requests on one connection, the second one closing it.
    let r = send(
        &server,
        "GET /a HTTP/1.1\r\nHost: x\r\n\r\nGET /b HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert!(r.contains("one") && r.contains("two"), "{r}");
    // HTTP/1.0 closes after one answer.
    let r = send(&server, "GET /a HTTP/1.0\r\n\r\n");
    assert!(r.ends_with("one") && r.contains("connection: close"), "{r}");
    // A chunked request body, with an extension and a trailer.
    let r = send(
        &server,
        "POST /c HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n7;ext=1\r\nchunked\r\n5\r\n body\r\n0\r\nTrailer: x\r\n\r\n",
    );
    assert_eq!(body_of(&r), "chunks", "{r}");
    // Expect: 100-continue gets its interim response.
    let r = send(
        &server,
        "POST /c HTTP/1.1\r\nHost: x\r\nExpect: 100-continue\r\nConnection: close\r\nContent-Length: 12\r\n\r\nchunked body",
    );
    assert!(
        r.starts_with("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK"),
        "{r}"
    );
    let started = Instant::now();
    get(&server, "/slow", "");
    assert!(started.elapsed() >= Duration::from_millis(140));
    // Garbage and early disconnects do not hurt the server.
    assert_eq!(send(&server, "nonsense\r\n\r\n"), "");
    assert_eq!(
        send(
            &server,
            "POST /c HTTP/1.1\r\nContent-Length: 999999999999\r\n\r\n"
        ),
        ""
    );
    assert_eq!(
        send(
            &server,
            "POST /c HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n"
        ),
        ""
    );
    assert_eq!(
        send(&server, "GET /a HTTP/1.1\r\nBroken header\r\n\r\n"),
        ""
    );
    assert_eq!(send(&server, ""), "");
    assert_eq!(send(&server, "\r\n\r\n"), "");
    assert!(get(&server, "/a", "").contains("one"));
}

/// A test that blocks its own (single-threaded) runtime while waiting for the answer, as one that waits for a
/// spawned process does, still gets one: the server has a thread of its own.
#[quench_test::test]
async fn answers_while_the_test_runtime_is_blocked() {
    let server = MockServer::start().await;
    Mock::given(path("/blocked"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    assert_eq!(body_of(&get(&server, "/blocked", "")), "ok");
}

quench_test::main!();
