//! Booting real servers: `serve_app` (plain and TLS with the HTTP redirect) and `serve`.
//!
//! Environment variables are process-global, so the tests take turns on `ENV_LOCK`.
#![allow(clippy::await_holding_lock)]

use quench_http::prelude::{ContainerBuilder, Endpoint};
use quench_starter::http::{discover_and_mount, serve, serve_app};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn fixtures() -> String {
    format!(
        "{}/../quench-tls/tests/fixtures",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Waits until `url` answers `GET` with a success status.
async fn wait_ok(client: &quench_client::transport::Client, url: &str) -> bool {
    for _ in 0..100 {
        if client
            .get(url)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn plain_http_when_there_is_no_certificate() {
    let _guard = ENV_LOCK.lock().unwrap();
    let port = free_port();
    quench_core::env::set("SERVER_ADDR", format!("127.0.0.1:{port}"));
    quench_core::env::set("SERVER_CERT_PATH", "/nonexistent/cert.pem");
    quench_core::env::set("SERVER_KEY_PATH", "/nonexistent/key.pem");
    let app: Arc<dyn Endpoint> = discover_and_mount("/");
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(quench_starter::common::health::HealthState::live())
            .build()
            .await
            .unwrap(),
    );
    let server = tokio::spawn(serve_app("plain-test", app, container));
    let client = quench_client::transport::Client::new();
    assert!(wait_ok(&client, &format!("http://127.0.0.1:{port}/health")).await);
    server.abort();
}

#[tokio::test]
async fn https_with_an_http_redirect_server() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (https_port, redirect_port) = (free_port(), free_port());
    quench_core::env::set("SERVER_ADDR", format!("127.0.0.1:{https_port}"));
    quench_core::env::set(
        "SERVER_HTTP_REDIRECT_ADDR",
        format!("127.0.0.1:{redirect_port}"),
    );
    quench_core::env::set("SERVER_CERT_PATH", format!("{}/server.pem", fixtures()));
    quench_core::env::set("SERVER_KEY_PATH", format!("{}/server.key", fixtures()));
    let app: Arc<dyn Endpoint> = discover_and_mount("/");
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(quench_starter::common::health::HealthState::live())
            .build()
            .await
            .unwrap(),
    );
    let server = tokio::spawn(serve_app("tls-test", app, container));

    let ca = std::fs::read(format!("{}/ca.pem", fixtures())).unwrap();
    let client = quench_client::transport::Client::builder()
        .add_root_certificate_pem(ca)
        .build()
        .unwrap();
    assert!(wait_ok(&client, &format!("https://localhost:{https_port}/health")).await);

    // The plain port answers every request with a permanent redirect to the HTTPS one.
    let no_follow = tokio::net::TcpStream::connect(("127.0.0.1", redirect_port))
        .await
        .unwrap();
    let mut stream = no_follow;
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"GET /health?x=1 HTTP/1.1\r\nHost: example.test:8080\r\nConnection: close\r\n\r\n",
    )
    .await
    .unwrap();
    let mut answer = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut answer)
        .await
        .unwrap();
    let answer = String::from_utf8_lossy(&answer).to_lowercase();
    assert!(answer.starts_with("http/1.1 308"), "{answer}");
    assert!(
        answer.contains(&format!(
            "location: https://example.test:{https_port}/health?x=1"
        )),
        "{answer}"
    );
    server.abort();
    quench_core::env::remove("SERVER_HTTP_REDIRECT_ADDR");
}

#[tokio::test]
async fn serve_builds_the_graph_and_reports_readiness() {
    let _guard = ENV_LOCK.lock().unwrap();
    let port = free_port();
    quench_core::env::set("SERVER_ADDR", format!("127.0.0.1:{port}"));
    quench_core::env::set("SERVER_CERT_PATH", "/nonexistent/cert.pem");
    quench_core::env::set("ALLOW_IN_MEMORY_DB", "true");
    quench_core::env::set("SERVICE_NAME", "boot");
    quench_core::env::set("BASE_PATH", "/");
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(serve(None, async move {
        let _ = ready_rx.await;
    }));
    let client = quench_client::transport::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    assert!(wait_ok(&client, &format!("{base}/health/live")).await);
    let not_ready = client
        .get(format!("{base}/health/ready"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        not_ready.status().as_u16(),
        503,
        "initialisation has not finished"
    );
    ready_tx.send(()).unwrap();
    assert!(wait_ok(&client, &format!("{base}/health/ready")).await);
    server.abort();
    quench_core::env::remove("ALLOW_IN_MEMORY_DB");
}
