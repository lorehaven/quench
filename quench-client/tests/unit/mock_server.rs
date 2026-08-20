//! Minimal raw-HTTP test double for exercising `quench_client`'s request
//! plumbing without pulling in a mocking crate (none is a workspace
//! dependency yet). Accepts a fixed number of connections in sequence,
//! captures each request line/headers/body, and writes back the same
//! canned response to all of them.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub head: String,
    pub body: String,
}

async fn read_full_request(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = if buf.is_empty() {
            stream.read(&mut chunk).await
        } else {
            match timeout(Duration::from_millis(100), stream.read(&mut chunk)).await {
                Ok(result) => result,
                Err(_) => break,
            }
        };
        match read {
            Ok(0) | Err(_) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

async fn handle_one(stream: &mut TcpStream, status_line: &str, body: &str) -> CapturedRequest {
    let raw = read_full_request(stream).await;
    let (head, request_body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("write response");
    let _ = stream.shutdown().await;

    CapturedRequest {
        method,
        path,
        head: head.to_string(),
        body: request_body.to_string(),
    }
}

/// Starts a listener on an ephemeral loopback port, accepts one connection,
/// responds with `status_line` (e.g. `"200 OK"`) and `body` as a JSON
/// payload, and hands back the base URL plus a handle to what it captured.
pub async fn serve_once(
    status_line: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<CapturedRequest>) {
    let (base_url, mut handle) = serve_n(1, status_line, body).await;
    let handle = tokio::spawn(async move { handle.recv().await.expect("one response") });
    (base_url, handle)
}

/// Same as [`serve_once`] but accepts `count` connections in sequence,
/// responding to each identically, and streams every captured request back
/// over the returned channel as it happens.
pub async fn serve_n(
    count: usize,
    status_line: &'static str,
    body: &'static str,
) -> (String, tokio::sync::mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback listener");
    let addr = listener.local_addr().expect("local addr");
    let base_url = format!("http://{addr}");

    let (tx, rx) = tokio::sync::mpsc::channel(count.max(1));
    tokio::spawn(async move {
        for _ in 0..count {
            let (mut stream, _) = listener.accept().await.expect("accept connection");
            let captured = handle_one(&mut stream, status_line, body).await;
            if tx.send(captured).await.is_err() {
                break;
            }
        }
    });

    (base_url, rx)
}
