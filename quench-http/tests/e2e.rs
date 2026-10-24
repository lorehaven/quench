//! The framework over real sockets: HTTP, HTTPS, WebSocket upgrades and server-sent events.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::async_trait;
use quench_core::stream;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::error::HttpError;
use quench_http::extract::FromRequest;
use quench_http::multipart::Multipart;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
#[cfg(feature = "tls")]
use quench_http::server::serve_https_on;
use quench_http::ws::{self, Message};
use quench_proto::h1::client::{ClientConfig, ClientConn, send_request};
use quench_proto::sse::Event;
#[cfg(feature = "tls")]
use quench_proto::tls::{ClientOptions, TlsConnector, client_config, server_name};
use quench_proto::{Method, StatusCode, Uri};
#[cfg(feature = "tls")]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

struct App;

#[async_trait]
impl Endpoint for App {
    async fn call(&self, mut req: Request) -> Response {
        match req.uri().path() {
            "/hello" => Response::text(StatusCode::OK, "hello over the wire"),
            "/echo" => {
                let body = req
                    .take_body()
                    .collect_limited(1 << 20)
                    .await
                    .unwrap_or_default();
                Response::from_bytes(StatusCode::OK, body)
            }
            "/sse" => Response::sse(stream::iter(vec![
                Event::new().data("one"),
                Event::new().event("tick").data("two"),
            ])),
            "/ws2" => match ws::WebSocketUpgrade::from_request(&mut req).await {
                Ok(upgrade) => upgrade
                    .config(ws::Config {
                        max_message_size: 1024,
                        max_frame_size: 1024,
                    })
                    .on_upgrade(|mut socket| async move {
                        if let Ok(Some(Message::Text(text))) = socket.recv().await {
                            let _ = socket.send(Message::Text(text.to_uppercase())).await;
                        }
                    }),
                Err(err) => err.into_response(),
            },
            "/ws" => ws::upgrade(&req, |mut socket| async move {
                while let Ok(Some(message)) = socket.recv().await {
                    match message {
                        Message::Text(text) => {
                            let _ = socket.send(Message::Text(format!("echo: {text}"))).await;
                        }
                        Message::Binary(data) => {
                            let _ = socket.send(Message::Binary(data)).await;
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            })
            .unwrap_or_else(HttpError::into_response),
            "/upload" => {
                let mut form = Multipart::from_request(&mut req).await.unwrap();
                let mut seen = Vec::new();
                while let Some(field) = form.next_field().await.unwrap() {
                    let name = field.name().unwrap_or("?").to_string();
                    let file = field.file_name().map(str::to_string);
                    let value = field.bytes().await.unwrap();
                    seen.push(format!(
                        "{name}:{}:{}",
                        file.unwrap_or_default(),
                        value.len()
                    ));
                }
                Response::text(StatusCode::OK, seen.join(","))
            }
            _ => Response::not_found(),
        }
    }
}

async fn start() -> (
    std::net::SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve_http_on(
        Arc::new(App),
        container,
        listener,
        async move {
            let _ = stopped.await;
        },
    ));
    (addr, stop, server)
}

fn url(addr: std::net::SocketAddr, path: &str) -> Uri {
    Uri::parse(&format!("http://{addr}{path}")).unwrap()
}

#[quench_test::test]
async fn plain_http_round_trip_and_connection_reuse() {
    let (addr, stop, server) = start().await;
    let mut conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    for _ in 0..3 {
        let request = quench_proto::Request::new(Method::GET, url(addr, "/hello"));
        let exchange = send_request(conn, request, &ClientConfig::default())
            .await
            .unwrap();
        assert_eq!(exchange.response.status, StatusCode::OK);
        assert_eq!(
            exchange.response.body.collect_limited(1000).await.unwrap(),
            "hello over the wire"
        );
        conn = exchange
            .reuse
            .await
            .expect("the server keeps the connection alive");
    }
    let request = quench_proto::Request::new(Method::POST, url(addr, "/echo")).body("posted bytes");
    let exchange = send_request(conn, request, &ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(
        exchange.response.body.collect_limited(1000).await.unwrap(),
        "posted bytes"
    );
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn unknown_paths_are_404s() {
    let (addr, stop, server) = start().await;
    let conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    let exchange = send_request(
        conn,
        quench_proto::Request::new(Method::GET, url(addr, "/nope")),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.status, StatusCode::NOT_FOUND);
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn server_sent_events_stream_in_order() {
    let (addr, stop, server) = start().await;
    let conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    let exchange = send_request(
        conn,
        quench_proto::Request::new(Method::GET, url(addr, "/sse")),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        exchange.response.headers.get("content-type").unwrap(),
        "text/event-stream"
    );
    let body = exchange.response.body.collect_limited(1000).await.unwrap();
    assert_eq!(body, "data: one\n\nevent: tick\ndata: two\n\n");
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn multipart_uploads_stream_through_the_server() {
    let (addr, stop, server) = start().await;
    let big = "x".repeat(300_000);
    let body = format!(
        "--b0undary\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n--b0undary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"big.txt\"\r\nContent-Type: text/plain\r\n\r\n{big}\r\n--b0undary--\r\n"
    );
    let request = quench_proto::Request::new(Method::POST, url(addr, "/upload"))
        .header("content-type", "multipart/form-data; boundary=b0undary")
        .body(body);
    let conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    let exchange = send_request(conn, request, &ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(
        exchange.response.body.collect_limited(1000).await.unwrap(),
        "note::2,file:big.txt:300000"
    );
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn websocket_upgrade_and_echo() {
    let (addr, stop, server) = start().await;
    let mut tcp = TcpStream::connect(addr).await.unwrap();
    let client_key = quench_proto::ws::generate_key();
    let handshake = format!(
        "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {client_key}\r\n\r\n"
    );
    tcp.write_all(handshake.as_bytes()).await.unwrap();
    // Read up to the end of the response head, leaving any frame bytes for the socket.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        tcp.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    assert!(
        head.starts_with("HTTP/1.1 101 Switching Protocols\r\n"),
        "{head}"
    );
    assert!(
        head.to_ascii_lowercase().contains(
            &format!(
                "sec-websocket-accept: {}",
                quench_proto::ws::accept_key(&client_key)
            )
            .to_ascii_lowercase()
        ),
        "{head}"
    );

    let mut socket = quench_proto::ws::client(tcp, quench_proto::ws::Config::default());
    socket.send(Message::Text("ping me".into())).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(Message::Text("echo: ping me".into()))
    );
    socket.send(Message::Binary(vec![9, 8, 7])).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(Message::Binary(vec![9, 8, 7]))
    );
    socket.close(1000, "bye").await.unwrap();
    assert!(matches!(
        socket.recv().await.unwrap(),
        Some(Message::Close(_))
    ));
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn a_plain_get_to_the_websocket_route_is_a_400() {
    let (addr, stop, server) = start().await;
    let conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    let exchange = send_request(
        conn,
        quench_proto::Request::new(Method::GET, url(addr, "/ws")),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.status, StatusCode::BAD_REQUEST);
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[cfg(feature = "tls")]
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quench-tls/tests/fixtures")
        .join(name)
}

#[cfg(feature = "tls")]
#[quench_test::test]
async fn https_with_a_real_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let config = quench_http::server::load_tls(fixture("server.pem"), fixture("server.key"))
        .expect("fixture credentials");
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve_https_on(
        Arc::new(App),
        container,
        listener,
        config,
        async move {
            let _ = stopped.await;
        },
    ));

    let options = ClientOptions {
        extra_roots_pem: vec![std::fs::read(fixture("ca.pem")).unwrap()],
        ..ClientOptions::default()
    };
    let connector = TlsConnector::new(client_config(&options).unwrap());
    let tcp = TcpStream::connect(addr).await.unwrap();
    let tls = connector
        .connect(server_name("localhost").unwrap(), tcp)
        .await
        .unwrap();
    let request =
        quench_proto::Request::new(Method::GET, Uri::parse("https://localhost/hello").unwrap());
    let exchange = send_request(ClientConn::new(tls), request, &ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(
        exchange.response.body.collect_limited(1000).await.unwrap(),
        "hello over the wire"
    );

    // A plaintext request to the TLS port just fails the handshake; the server stays up.
    let mut plain = TcpStream::connect(addr).await.unwrap();
    plain
        .write_all(b"GET /hello HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut sink = Vec::new();
    let _ = plain.read_to_end(&mut sink).await;

    let tcp = TcpStream::connect(addr).await.unwrap();
    let tls = connector
        .connect(server_name("localhost").unwrap(), tcp)
        .await
        .unwrap();
    let request =
        quench_proto::Request::new(Method::GET, Uri::parse("https://localhost/hello").unwrap());
    let exchange = send_request(ClientConn::new(tls), request, &ClientConfig::default())
        .await
        .unwrap();
    assert_eq!(exchange.response.status, StatusCode::OK);
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[quench_test::test]
async fn shutdown_returns_promptly_with_idle_keep_alive_connections_open() {
    let (addr, stop, server) = start().await;
    let _idle = TcpStream::connect(addr).await.unwrap();
    let mut conn = TcpStream::connect(addr).await.unwrap();
    conn.write_all(b"GET /hello HTTP/1.1\r\nHost: x\r\n\r\n")
        .await
        .unwrap();
    let mut buf = [0u8; 1024];
    let n = conn.read(&mut buf).await.unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).contains("200 OK"));
    let _ = stop.send(());
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), server).await;
    assert!(
        finished.is_ok(),
        "idle connections must not hold up the drain"
    );
}

#[quench_test::test]
async fn the_websocket_extractor_upgrades_and_rejects() {
    let (addr, stop, server) = start().await;
    // A plain GET is refused by the extractor with a 400.
    let conn = ClientConn::new(TcpStream::connect(addr).await.unwrap());
    let exchange = send_request(
        conn,
        quench_proto::Request::new(Method::GET, url(addr, "/ws2")),
        &ClientConfig::default(),
    )
    .await
    .unwrap();
    assert_eq!(exchange.response.status, StatusCode::BAD_REQUEST);

    let mut tcp = TcpStream::connect(addr).await.unwrap();
    let key = quench_proto::ws::generate_key();
    let handshake = format!(
        "GET /ws2 HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n\r\n"
    );
    tcp.write_all(handshake.as_bytes()).await.unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        tcp.read_exact(&mut byte).await.unwrap();
        head.push(byte[0]);
    }
    let mut socket = quench_proto::ws::client(tcp, ws::Config::default());
    socket.send(Message::Text("shout".into())).await.unwrap();
    assert_eq!(
        socket.recv().await.unwrap(),
        Some(Message::Text("SHOUT".into()))
    );
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

quench_test::main!();
