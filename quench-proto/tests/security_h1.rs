//! Adversarial HTTP/1.1 server tests: request smuggling, header injection, resource exhaustion.
//!
//! Every case feeds hostile wire bytes to a real `serve_connection` and checks two things: what the
//! client is told, and - the part that matters for smuggling - what the *handler* saw. A smuggled
//! request is, by definition, a request the handler sees that the front-end never meant to forward,
//! so each ambiguous input must reach the handler zero times (rejected) or exactly the number of
//! times a strict parser would agree on.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_proto::h1::parse::Limits;
use quench_proto::h1::server::{ServerConfig, Shutdown, serve_connection};
use quench_proto::{Request, Response, StatusCode};
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What the handler observed, one entry per invocation: `"METHOD path body"`.
type Seen = Arc<Mutex<Vec<String>>>;

struct Outcome {
    wire: String,
    seen: Vec<String>,
}

impl Outcome {
    fn statuses(&self) -> Vec<u16> {
        self.wire
            .split("HTTP/1.1 ")
            .skip(1)
            .filter_map(|s| s.get(..3)?.parse().ok())
            .collect()
    }

    fn assert_rejected(&self, case: &str) {
        let statuses = self.statuses();
        assert!(
            self.seen.is_empty(),
            "{case}: handler must not run, saw {:?}\n{}",
            self.seen,
            self.wire
        );
        // Either an explicit error that closes the connection, or a silent hang-up (nothing sent):
        // both leave the attacker without a live connection to smuggle on.
        assert!(
            statuses.iter().all(|s| (400..600).contains(s)),
            "{case}: expected only 4xx/5xx, got {statuses:?}\n{}",
            self.wire
        );
        assert!(
            statuses.is_empty() || self.wire.to_ascii_lowercase().contains("connection: close"),
            "{case}: a framing error must close the connection\n{}",
            self.wire
        );
    }
}

fn quick() -> ServerConfig {
    ServerConfig {
        header_read_timeout: Duration::from_millis(400),
        idle_timeout: Duration::from_millis(400),
        write_timeout: Duration::from_millis(500),
        drain_timeout: Duration::from_millis(200),
        ..ServerConfig::default()
    }
}

async fn run_with(config: ServerConfig, wire: &[u8]) -> Outcome {
    let seen: Seen = Arc::default();
    let recorder = seen.clone();
    let service = move |mut req: Request| {
        let recorder = recorder.clone();
        async move {
            let body = std::mem::take(&mut req.body)
                .collect_limited(1 << 20)
                .await
                .unwrap_or_default();
            recorder.lock().unwrap().push(format!(
                "{} {} {}",
                req.method.as_str(),
                req.uri.path(),
                String::from_utf8_lossy(&body)
            ));
            Response::new(StatusCode::OK).body("ok")
        }
    };
    let (mut client, server) = tokio::io::duplex(1 << 22);
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    // The server may legitimately hang up before reading everything (oversized heads); a failed
    // write is the defence working, not a test failure.
    let _ = client.write_all(wire).await;
    let _ = client.shutdown().await;
    let mut out = Vec::new();
    let _ = client.read_to_end(&mut out).await;
    let _ = task.await;
    let wire = String::from_utf8_lossy(&out).into_owned();
    let seen = seen.lock().unwrap().clone();
    Outcome { wire, seen }
}

async fn run(wire: &[u8]) -> Outcome {
    run_with(quick(), wire).await
}

// --- request smuggling: framing ambiguity must be refused, not interpreted ---

#[quench_test::test]
async fn content_length_with_transfer_encoding_is_rejected() {
    // CL.TE / TE.CL: the classic desync. The trailing GET is what a front-end would not have seen.
    let wire =
        b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n\
                 0\r\n\r\nGET /admin HTTP/1.1\r\nHost: x\r\n\r\n";
    run(wire).await.assert_rejected("CL+TE");
}

#[quench_test::test]
async fn conflicting_duplicate_content_lengths_are_rejected() {
    let wire = b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\nContent-Length: 40\r\n\r\nabc\
                 GET /admin HTTP/1.1\r\nHost: x\r\n\r\n";
    run(wire).await.assert_rejected("dup CL");
}

#[quench_test::test]
async fn malformed_content_lengths_are_rejected() {
    for (case, value) in [
        ("negative", "-1"),
        ("plus sign", "+3"),
        ("hex", "0x3"),
        ("trailing junk", "3 junk"),
        ("overflow u64", "18446744073709551616"),
        ("empty", ""),
        ("float", "3.0"),
        ("unicode digit", "٣"),
    ] {
        let wire = format!("POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: {value}\r\n\r\nabc")
            .into_bytes();
        run(&wire).await.assert_rejected(case);
    }
}

#[quench_test::test]
async fn identical_content_length_list_is_read_as_one_value() {
    // RFC 9110 5.3 lets a recipient fold `3, 3` into `3`; what it must never do is read it as a
    // different length than the front-end did. Differing members (`3, 40`) are the attack.
    let ok =
        run(b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 3, 3\r\nConnection: close\r\n\r\nabc")
            .await;
    assert_eq!(ok.seen, vec!["POST /a abc".to_string()], "{}", ok.wire);
    run(b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 3, 40\r\n\r\nabc")
        .await
        .assert_rejected("differing CL list");
}

#[quench_test::test]
async fn obfuscated_transfer_encoding_is_never_treated_as_chunked_or_ignored() {
    // Each variant is one a lenient front-end might read as chunked while the back-end ignores it
    // (or vice versa). The only safe answers are "reject" or "treat as exactly chunked".
    for (case, header) in [
        ("xchunked", "Transfer-Encoding: xchunked"),
        ("space before colon", "Transfer-Encoding : chunked"),
        ("tab prefix", "Transfer-Encoding:\tchunked"),
        ("identity,chunked", "Transfer-Encoding: identity, chunked"),
        ("chunked,identity", "Transfer-Encoding: chunked, identity"),
        ("double chunked", "Transfer-Encoding: chunked, chunked"),
        ("gzip", "Transfer-Encoding: gzip"),
        ("empty", "Transfer-Encoding:"),
        ("vertical tab", "Transfer-Encoding: \u{b}chunked"),
        ("cased value", "Transfer-Encoding: ChUnKeD"),
    ] {
        let wire = format!(
            "POST /a HTTP/1.1\r\nHost: x\r\n{header}\r\nContent-Length: 6\r\n\r\n0\r\n\r\nGET /admin HTTP/1.1\r\nHost: x\r\n\r\n"
        )
        .into_bytes();
        let out = run(&wire).await;
        assert!(
            !out.seen.iter().any(|s| s.contains("/admin")),
            "{case}: smuggled GET /admin reached the handler: {:?}\n{}",
            out.seen,
            out.wire
        );
    }
}

#[quench_test::test]
async fn te_without_content_length_does_not_leak_a_second_request() {
    let wire = b"POST /a HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: xchunked\r\n\r\n\
                 GET /admin HTTP/1.1\r\nHost: x\r\n\r\n";
    let out = run(wire).await;
    assert!(
        !out.seen.iter().any(|s| s.contains("/admin")),
        "{:?}\n{}",
        out.seen,
        out.wire
    );
}

#[quench_test::test]
async fn bare_lf_line_endings_are_rejected() {
    // A bare LF is a line end to some parsers and body data to others - the lever for header smuggling.
    run(b"GET /a HTTP/1.1\nHost: x\n\n")
        .await
        .assert_rejected("bare LF");
    run(b"GET /a HTTP/1.1\r\nHost: x\nX-Y: z\r\n\r\n")
        .await
        .assert_rejected("bare LF mid-head");
}

#[quench_test::test]
async fn obs_fold_continuation_lines_are_rejected() {
    run(b"GET /a HTTP/1.1\r\nHost: x\r\nX-A: b\r\n folded\r\n\r\n")
        .await
        .assert_rejected("obs-fold");
    run(b"GET /a HTTP/1.1\r\nHost: x\r\nX-A: b\r\n\tfolded\r\n\r\n")
        .await
        .assert_rejected("obs-fold tab");
}

#[quench_test::test]
async fn control_characters_in_headers_are_rejected() {
    for (case, header) in [
        ("NUL in value", "X-A: a\0b"),
        ("CR in value", "X-A: a\rb"),
        ("NUL in name", "X-\0A: b"),
        ("space in name", "X A: b"),
        ("empty name", ": b"),
        ("no colon", "X-A b"),
        ("DEL in value", "X-A: a\x7fb"),
        ("bell in value", "X-A: a\x07b"),
    ] {
        let wire = format!("GET /a HTTP/1.1\r\nHost: x\r\n{header}\r\n\r\n").into_bytes();
        run(&wire).await.assert_rejected(case);
    }
}

#[quench_test::test]
async fn malformed_request_lines_are_rejected() {
    for (case, line) in [
        ("empty", ""),
        ("method only", "GET"),
        ("no version", "GET /a"),
        ("extra token", "GET /a HTTP/1.1 extra"),
        ("double space", "GET  /a HTTP/1.1"),
        ("NUL in target", "GET /a\0b HTTP/1.1"),
        ("space in target", "GET /a b HTTP/1.1"),
        ("tab separator", "GET\t/a\tHTTP/1.1"),
        ("http/0.9", "GET /a"),
        ("http/3", "GET /a HTTP/3.0"),
        ("garbage version", "GET /a HTTP/x.y"),
        ("lowercase version", "GET /a http/1.1"),
        ("binary method", "\u{1}\u{2} /a HTTP/1.1"),
    ] {
        let wire = format!("{line}\r\nHost: x\r\n\r\n").into_bytes();
        run(&wire).await.assert_rejected(case);
    }
}

#[quench_test::test]
async fn chunked_bodies_with_hostile_sizes_are_rejected() {
    for (case, chunks) in [
        ("overflowing size", "FFFFFFFFFFFFFFFFFF\r\nx\r\n0\r\n\r\n"),
        ("negative size", "-5\r\nhello\r\n0\r\n\r\n"),
        ("non-hex size", "zz\r\nhello\r\n0\r\n\r\n"),
        ("missing chunk CRLF", "5\r\nhelloXX0\r\n\r\n"),
        ("size with space", "5 5\r\nhello\r\n0\r\n\r\n"),
        ("empty size", "\r\nhello\r\n0\r\n\r\n"),
        ("bare LF size line", "5\nhello\r\n0\r\n\r\n"),
    ] {
        let wire = format!(
            "POST /a HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n{chunks}GET /admin HTTP/1.1\r\nHost: x\r\n\r\n"
        )
        .into_bytes();
        let out = run(&wire).await;
        assert!(
            !out.seen.iter().any(|s| s.contains("/admin")),
            "{case}: smuggled request reached the handler: {:?}\n{}",
            out.seen,
            out.wire
        );
    }
}

#[quench_test::test]
async fn pipelined_requests_after_a_bad_one_are_never_executed() {
    let wire = b"GET /ok HTTP/1.1\r\nHost: x\r\n\r\n\
                 GET /bad HTTP/1.1\r\nHost: x\r\nX-A: a\0b\r\n\r\n\
                 GET /after HTTP/1.1\r\nHost: x\r\n\r\n";
    let out = run(wire).await;
    assert_eq!(out.seen, vec!["GET /ok ".to_string()], "{}", out.wire);
    assert_eq!(out.statuses().first(), Some(&200), "{}", out.wire);
    assert!(out.statuses().contains(&400), "{}", out.wire);
}

#[quench_test::test]
async fn request_body_is_not_reinterpreted_as_the_next_request() {
    // A body that *looks* like a request must stay body data: one handler call, body intact.
    let inner = "GET /admin HTTP/1.1\r\nHost: x\r\n\r\n";
    let wire = format!(
        "POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{inner}",
        inner.len()
    )
    .into_bytes();
    let out = run(&wire).await;
    assert_eq!(out.seen.len(), 1, "{:?}\n{}", out.seen, out.wire);
    assert!(
        out.seen[0].starts_with("POST /a GET /admin"),
        "{:?}",
        out.seen
    );
}

// --- resource exhaustion ---

#[quench_test::test]
async fn oversized_heads_are_refused_without_running_the_handler() {
    let big_value = "a".repeat(70 * 1024);
    let wire = format!("GET /a HTTP/1.1\r\nHost: x\r\nX-Big: {big_value}\r\n\r\n").into_bytes();
    let out = run(&wire).await;
    assert!(out.seen.is_empty(), "{:?}", out.seen);
    assert!(out.statuses().iter().all(|s| *s >= 400), "{}", out.wire);

    let long_target = "a".repeat(9 * 1024);
    let wire = format!("GET /{long_target} HTTP/1.1\r\nHost: x\r\n\r\n").into_bytes();
    let out = run(&wire).await;
    assert!(out.seen.is_empty(), "{:?}", out.seen);
    assert!(out.statuses().iter().all(|s| *s >= 400), "{}", out.wire);
}

#[quench_test::test]
async fn too_many_headers_are_refused() {
    let headers = (0..300).fold(String::new(), |mut out, i| {
        let _ = write!(out, "X-{i}: v\r\n");
        out
    });
    let wire = format!("GET /a HTTP/1.1\r\nHost: x\r\n{headers}\r\n").into_bytes();
    let out = run(&wire).await;
    assert!(out.seen.is_empty(), "{:?}", out.seen);
    assert!(out.statuses().iter().all(|s| *s >= 400), "{}", out.wire);
}

#[quench_test::test]
async fn header_floods_stay_within_the_configured_limits() {
    let config = ServerConfig {
        limits: Limits {
            max_head_bytes: 512,
            max_headers: 5,
            max_uri_bytes: 64,
        },
        ..quick()
    };
    let wire = format!(
        "GET /a HTTP/1.1\r\nHost: x\r\nX-A: {}\r\n\r\n",
        "a".repeat(600)
    )
    .into_bytes();
    let out = run_with(config, &wire).await;
    assert!(out.seen.is_empty(), "{:?}", out.seen);
}

#[quench_test::test]
async fn slowloris_headers_are_cut_off_by_the_read_timeout() {
    let (mut client, server) = tokio::io::duplex(4096);
    let config = quick();
    let service = |_req: Request| async { Response::new(StatusCode::OK).body("ok") };
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    let started = Instant::now();
    // Drip one byte at a time, never finishing the head.
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: x\r\n")
        .await
        .unwrap();
    for _ in 0..40 {
        if client.write_all(b"X").await.is_err() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server must give up on a stalled head instead of holding the slot");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "took {:?} to drop a slowloris client",
        started.elapsed()
    );
}

#[quench_test::test]
async fn an_idle_keep_alive_connection_is_reaped() {
    let (mut client, server) = tokio::io::duplex(4096);
    let config = quick();
    let service = |_req: Request| async { Response::new(StatusCode::OK).body("ok") };
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .expect("idle connection must be closed by the idle timeout")
        .unwrap()
        .ok();
}

#[quench_test::test]
async fn a_client_that_never_reads_cannot_pin_the_connection() {
    // Tiny pipe + large response + a client that never reads: the write timeout has to free the task.
    let (client, server) = tokio::io::duplex(256);
    let config = quick();
    let service =
        |_req: Request| async { Response::new(StatusCode::OK).body(vec![b'z'; 4 * 1024 * 1024]) };
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    let mut client = client;
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("write timeout must release a connection whose peer stopped reading")
        .unwrap()
        .ok();
    drop(client);
}

#[quench_test::test]
async fn a_declared_huge_body_is_not_buffered_by_the_server() {
    // The server must not allocate the declared length up front: claim 1 TiB, send 3 bytes, close.
    let wire = b"POST /a HTTP/1.1\r\nHost: x\r\nContent-Length: 1099511627776\r\n\r\nabc";
    let started = Instant::now();
    let out = run(wire).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
    // Whatever the handler made of a truncated body, it must not have been handed a full one.
    assert!(out.seen.iter().all(|s| s.len() < 1024), "{:?}", out.seen);
}

// --- response splitting: the server must not let handler data forge extra messages ---

#[quench_test::test]
async fn crlf_in_a_response_header_value_cannot_split_the_response() {
    let (mut client, server) = tokio::io::duplex(1 << 16);
    let config = quick();
    let service = |_req: Request| async {
        Response::new(StatusCode::OK)
            .header("x-reflected", "a\r\nSet-Cookie: session=evil")
            .body("ok")
    };
    let task = tokio::spawn(async move {
        serve_connection(server, &service, &config, Shutdown::never()).await
    });
    client
        .write_all(b"GET /a HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut out = Vec::new();
    let _ = client.read_to_end(&mut out).await;
    let _ = task.await;
    let text = String::from_utf8_lossy(&out).to_ascii_lowercase();
    assert!(
        !text.contains("\r\nset-cookie: session=evil"),
        "header value injected a header line:\n{text}"
    );
}

quench_test::main!();
