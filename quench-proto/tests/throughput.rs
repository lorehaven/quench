//! A throughput smoke test, run on demand:
//! `cargo test --release -p quench-proto --test throughput -- --ignored --nocapture`.
//! It asserts only a very low floor; the printed numbers are for eyeballing regressions.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_proto::h1::client::{ClientConfig, ClientConn, send_request};
use quench_proto::h1::server::{ServerConfig, Shutdown, serve_connection};
use quench_proto::{Method, Request, Response, StatusCode, Uri};
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};

async fn hello(_req: Request) -> Response {
    Response::new(StatusCode::OK)
        .header("content-type", "text/plain")
        .body("hello world")
}

#[quench_test::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "run on demand"]
async fn keep_alive_requests_per_second() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = stream.set_nodelay(true);
            tokio::spawn(async move {
                let _ =
                    serve_connection(stream, &hello, &ServerConfig::default(), Shutdown::never())
                        .await;
            });
        }
    });

    let per_connection = 20_000;
    let connections = 8;
    let started = Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..connections {
        tasks.push(tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            stream.set_nodelay(true).unwrap();
            let mut conn = ClientConn::new(stream);
            let uri = Uri::parse(&format!("http://{addr}/")).unwrap();
            for _ in 0..per_connection {
                let exchange = send_request(
                    conn,
                    Request::new(Method::GET, uri.clone()),
                    &ClientConfig::default(),
                )
                .await
                .unwrap();
                let body = exchange.response.body.collect_limited(1024).await.unwrap();
                assert_eq!(body, "hello world");
                conn = exchange.reuse.await.unwrap();
            }
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    let secs = started.elapsed().as_secs_f64();
    let total = f64::from(per_connection * connections);
    println!(
        "{connections} connections x {per_connection} requests: {:.0} req/s",
        total / secs
    );
    assert!(total / secs > 1_000.0);
}

quench_test::main!();
