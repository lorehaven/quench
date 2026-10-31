# quench-proto
HTTP types, an HTTP/1.1 codec, cookies, WebSocket framing and a thin rustls glue, shared by `quench-http` and `quench-client`.

## What it is
`quench-proto` is the protocol layer under `quench-http` (server) and `quench-client`: the HTTP message types, a strict HTTP/1.1 codec with connection drivers for both ends, cookies, `multipart/form-data`, server-sent events, WebSocket (RFC 6455) and a re-export of the TLS glue. It replaces `http`, `http-body-util`, `hyper`, `hyper-util`, `multer` and `cookie`.

Despite its name it has nothing to do with Protocol Buffers. It is HTTP/1.1 only, and parsing is strict: anything that could make two parsers disagree about where a message ends is rejected. It runs on `tokio`.

## Use cases
- Serve HTTP/1.1 on a TCP listener with any `async fn(Request) -> Response`.
- Call a server with a single request and a size-limited body read.
- Parse multipart uploads, cookies and server-sent events.
- Speak WebSocket framing and the handshake.

## Example
```rust
use quench_proto::h1::server::{serve_connection, shutdown_channel, ServerConfig};
use quench_proto::{Request, Response, StatusCode};
use std::sync::Arc;
use tokio::net::TcpListener;

async fn app(req: Request) -> Response {
    match req.uri.path() {
        "/" => Response::new(StatusCode::OK).body("home"),
        _ => Response::new(StatusCode::NOT_FOUND),
    }
}

let listener = TcpListener::bind("127.0.0.1:0").await?;
let (stop, shutdown) = shutdown_channel();
let config = Arc::new(ServerConfig::default());
let (tcp, _) = listener.accept().await?;
let _ = serve_connection(tcp, &app, &config, shutdown).await;
```

## Features
- `server`, `client`: the HTTP/1.1 connection drivers.
- `ws`: WebSocket framing and handshake.
- `multipart`, `sse`: `multipart/form-data` and server-sent events.
- `tls`: re-exports `quench-tls`.
- All are on by default; the message types, cookies and the codec are always built.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-proto/
