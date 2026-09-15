//! `Response::streaming` is what an SSE endpoint (switchboard's GPU/vLLM
//! status streams, ported from actix-web's `.streaming()`) needs: a
//! chunked body driven by an item stream rather than a fixed buffer.
//! Proved over a real socket, since chunked framing is a wire-level
//! concern `Endpoint::call` dispatch alone can't exercise.

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use std::convert::Infallible;
use std::io::{Read, Write};
use std::net::TcpStream as StdTcpStream;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

struct SseEndpoint;

#[async_trait]
impl Endpoint for SseEndpoint {
    async fn call(&self, _req: Request) -> Response {
        let items = vec![
            Ok::<Bytes, Infallible>(Bytes::from_static(b"event: status\ndata: one\n\n")),
            Ok(Bytes::from_static(b"event: status\ndata: two\n\n")),
            Ok(Bytes::from_static(b"event: status\ndata: three\n\n")),
        ];
        Response::streaming(http::StatusCode::OK, stream::iter(items))
            .header("content-type", "text/event-stream")
    }
}

fn read_all(stream: &mut StdTcpStream, timeout: Duration) -> String {
    stream.set_read_timeout(Some(timeout)).unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("read failed: {e}"),
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
async fn streamed_items_arrive_as_chunked_body_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(SseEndpoint);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let shutdown = async move {
        let _ = shutdown_rx.await;
    };
    let server = tokio::spawn(serve_http_on(app, container, listener, shutdown));

    let response = tokio::task::spawn_blocking(move || {
        let mut stream = StdTcpStream::connect(addr).unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        read_all(&mut stream, Duration::from_secs(5))
    })
    .await
    .unwrap();

    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "{response:?}");
    assert!(
        response.contains("content-type: text/event-stream"),
        "{response:?}"
    );
    assert!(
        response.contains("transfer-encoding: chunked"),
        "{response:?}"
    );
    assert!(
        response.contains("event: status\ndata: one\n\n"),
        "{response:?}"
    );
    assert!(
        response.contains("event: status\ndata: two\n\n"),
        "{response:?}"
    );
    assert!(
        response.contains("event: status\ndata: three\n\n"),
        "{response:?}"
    );
}

#[tokio::test]
async fn a_mid_stream_error_ends_the_response_instead_of_hanging() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    struct FailsOnSecondItem;

    #[async_trait]
    impl Endpoint for FailsOnSecondItem {
        async fn call(&self, _req: Request) -> Response {
            let items: Vec<Result<Bytes, std::io::Error>> = vec![
                Ok(Bytes::from_static(b"first")),
                Err(std::io::Error::other("producer closed")),
            ];
            Response::streaming(http::StatusCode::OK, stream::iter(items))
        }
    }

    let resp = FailsOnSecondItem.call(Request::new(
        http::Method::GET,
        "/".parse().unwrap(),
        http::HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container,
    ));
    let resp = resp.await;
    assert_eq!(resp.status(), http::StatusCode::OK);

    // Collecting the body should surface the error rather than hang -
    // proves the mapped error actually propagates through `BoxBody`
    // instead of being silently swallowed.
    use http_body_util::BodyExt;
    let collected = resp.into_hyper().into_body().collect().await;
    assert!(
        collected.is_err(),
        "expected the mid-stream error to surface"
    );
}
