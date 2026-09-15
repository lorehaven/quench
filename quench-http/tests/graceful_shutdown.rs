//! Drives `serve_http_on` over a real TCP socket with a real HTTP/1.1
//! client (hand-rolled, to avoid a new dependency) - this is deliberately
//! the one test in this crate that doesn't go through `Endpoint::call`
//! directly, because the thing under test (does an in-flight request
//! survive a shutdown signal, does a new connection get refused once
//! draining is done) only exists at the socket layer.

use async_trait::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use std::io::{Read, Write};
use std::net::TcpStream as StdTcpStream;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

/// Answers after a delay, so a test can trigger shutdown while a request
/// is provably still in flight.
struct SlowEcho {
    delay: Duration,
}

#[async_trait]
impl Endpoint for SlowEcho {
    async fn call(&self, _req: Request) -> Response {
        tokio::time::sleep(self.delay).await;
        Response::text(http::StatusCode::OK, "slow-ok")
    }
}

fn read_http_response(stream: &mut StdTcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                // Good enough for this test's small, fixed bodies: stop once
                // we've plausibly got a full response rather than trying to
                // parse Content-Length.
                if buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() > 50 {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("read failed: {e}"),
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
async fn in_flight_request_survives_shutdown_and_new_connections_are_then_refused() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(SlowEcho {
        delay: Duration::from_millis(300),
    });

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown = async move {
        let _ = shutdown_rx.await;
    };

    let server = tokio::spawn(serve_http_on(app, container, listener, shutdown));

    // Open the in-flight connection and send the request, but don't block
    // reading the response yet - the handler is still sleeping.
    let mut in_flight = tokio::task::spawn_blocking(move || {
        let mut stream = StdTcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        stream
    })
    .await
    .unwrap();

    // Give the server a moment to accept and start the handler, then signal
    // shutdown while that request is still sleeping inside `SlowEcho`.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = shutdown_tx.send(());

    // A connection attempt after the shutdown signal was sent - whether it
    // lands before or after the listener actually stops accepting is a
    // race, but either a refused connection or (if it slips in just before
    // the listener closes) a normal response are both correct; what would
    // be wrong is the *first* connection getting cut off.
    let _ = StdTcpStream::connect(addr);

    let response = tokio::task::spawn_blocking(move || read_http_response(&mut in_flight))
        .await
        .unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "in-flight request should complete normally: {response:?}"
    );
    // Chunked transfer-encoding wraps the body (`7\r\nslow-ok\r\n0\r\n\r\n`), so
    // check for the real body rather than an exact tail match.
    assert!(
        response.contains("slow-ok"),
        "in-flight request should get its real body: {response:?}"
    );

    // `serve_http_on` should return promptly once its one in-flight
    // connection finished draining, not sit around for the full
    // `DEFAULT_GRACEFUL_TIMEOUT`.
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("serve_http_on should return once draining is done")
        .unwrap()
        .unwrap();

    // Now that the server task has returned, the listener is closed - a
    // fresh connection attempt has to fail.
    let refused = StdTcpStream::connect(addr);
    assert!(
        refused.is_err(),
        "listener should be closed after shutdown completes"
    );
}
