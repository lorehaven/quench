//! Unit tests for `common/wait.rs`.

#![allow(unsafe_code)] // tests mutate the process environment (`set_var` is unsafe in edition 2024)

use quench_starter::common::wait::{gatehouse_health_url, wait_for_services};
use std::sync::Mutex;
use std::time::Duration;

/// Both env-reading tests below mutate `GATEHOUSE_URL`.
static ENV_LOCK: Mutex<()> = Mutex::new(());
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[test]
fn gatehouse_health_url_appends_the_ready_path_and_strips_a_trailing_slash() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    unsafe {
        std::env::set_var("GATEHOUSE_URL", "https://gatehouse.internal/");
    }
    assert_eq!(
        gatehouse_health_url(),
        "https://gatehouse.internal/health/ready"
    );
    unsafe {
        std::env::remove_var("GATEHOUSE_URL");
    }
}

#[test]
fn gatehouse_health_url_defaults_to_a_bare_ready_path_when_unset() {
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    unsafe {
        std::env::remove_var("GATEHOUSE_URL");
    }
    assert_eq!(gatehouse_health_url(), "/health/ready");
}

#[tokio::test]
async fn wait_for_services_returns_once_the_dependency_answers_successfully() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a free port");
    let addr = listener.local_addr().expect("a local address");

    let server_task = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let _ = socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        }
    });

    let url = format!("http://{addr}/health/ready");
    tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_services("test-service", vec![url.as_str()]),
    )
    .await
    .expect("wait_for_services should resolve once the dependency is healthy");

    server_task.abort();
}
