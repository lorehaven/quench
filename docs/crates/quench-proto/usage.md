---
title: Usage
parent: quench-proto
nav_order: 1
---

Most services use this crate through `quench-http`/`quench-client`. These examples use it directly.

## 1. Serve HTTP/1.1 on a TCP listener

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

# async fn run() -> std::io::Result<()> {
let listener = TcpListener::bind("127.0.0.1:0").await?;
let (stop, shutdown) = shutdown_channel();
let config = Arc::new(ServerConfig::default());
loop {
    let (tcp, _) = listener.accept().await?;
    let (config, shutdown) = (config.clone(), shutdown.clone());
    tokio::spawn(async move {
        let _ = serve_connection(tcp, &app, &config, shutdown).await;
    });
    # break;
}
stop.signal(); // connections finish the request in flight and stop; idle ones exit at once
# Ok(()) }
```

Any `async fn(Request) -> Response` (or closure) is a `Service`. The driver adds `Date`, `Content-Length` or `Transfer-Encoding: chunked`, and `Connection: close` as needed.

## 2. Call a server

```rust
use quench_proto::h1::client::{send_request, ClientConfig, ClientConn};
use quench_proto::{Method, Request, Uri};
use tokio::net::TcpStream;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let tcp = TcpStream::connect("127.0.0.1:8080").await?;
let request = Request::new(Method::GET, Uri::parse("http://127.0.0.1:8080/health")?)
    .header("accept", "application/json");
let exchange = send_request(ClientConn::new(tcp), request, &ClientConfig::default()).await?;
println!("{}", exchange.response.status);
let body = exchange.response.body.collect_limited(1 << 20).await?;   // at most 1 MiB
// exchange.reuse yields the connection back if the peer allows keep-alive
# let _ = body; Ok(()) }
```

`send_request` sets `Host` (authority without userinfo) when the request has none. Interim `1xx` responses are skipped.

## 3. Read a request body safely

```rust
# use quench_proto::{Request, Response, StatusCode};
async fn upload(req: Request) -> Response {
    match req.body.collect_limited(64 * 1024).await {
        Ok(bytes) => Response::new(StatusCode::OK).body(format!("{} bytes", bytes.len())),
        Err(quench_proto::body::CollectError::TooLarge(limit)) =>
            Response::new(StatusCode::PAYLOAD_TOO_LARGE).body(format!("max {limit}")),
        Err(_) => Response::new(StatusCode::BAD_REQUEST),
    }
}
```

The server driver does not cap the request body for the handler; the limit is the handler's job. A handler that ignores its body does not wedge a keep-alive connection (the unread rest is discarded up to `ServerConfig::drain_limit`, else the response carries `Connection: close`).

## 4. Cookies

```rust
use quench_proto::cookie::{cookie_value, Cookie, SameSite};

let c = Cookie::build(("sid", "abc")).path("/").http_only(true).same_site(SameSite::Lax).secure(true).build();
assert_eq!(c.to_string(), "sid=abc; HttpOnly; SameSite=Lax; Secure; Path=/");
assert_eq!(cookie_value("a=1; sid=abc; b=2", "sid"), Some("abc"));
let parsed = Cookie::parse("k=v; Max-Age=60; Path=/x").unwrap();
assert_eq!(parsed.path(), Some("/x"));
```

Nothing is percent-encoded: keep names and values to cookie-safe characters.

## 5. Parse `multipart/form-data`

```rust
use quench_core::bytes::Bytes;
use quench_core::stream;
use quench_proto::multipart::{parse_boundary, Limits, Multipart};

# async fn run() -> Result<(), quench_proto::multipart::Error> {
let boundary = parse_boundary("multipart/form-data; boundary=XYZ")?;
let body = "--XYZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nhello\r\n--XYZ--\r\n";
let chunks = stream::iter(vec![Ok::<_, std::convert::Infallible>(Bytes::from(body))]);
let mut mp = Multipart::new(chunks, &boundary, Limits::default());
while let Some(field) = mp.next_field().await? {
    assert_eq!(field.name(), Some("a"));
    assert_eq!(field.text().await?, "hello");
}
# Ok(()) }
```

`Multipart::new` accepts any `Stream<Item = Result<Bytes, E>> + Unpin` where `E: Into<BoxError>` (here `quench_core::stream::iter` over one chunk). The snippets on this page were compiled against the crate while writing it (the server and client ones against a local socket).

## 6. Server-sent events

```rust
use quench_proto::sse::{response, Event};

let events = quench_core::stream::iter(vec![Event::new().event("tick").data("1"), Event::new().comment("")]);
let resp = response(events);
assert_eq!(resp.headers.get("content-type").unwrap(), "text/event-stream");
assert_eq!(Event::new().event("tick").id("7").data("a\nb").to_string(), "event: tick\nid: 7\ndata: a\ndata: b\n\n");
```

The response is `200` with `content-type: text/event-stream`, `cache-control: no-cache` and `x-accel-buffering: no`.

## 7. Upgrade to WebSocket

```rust
use quench_proto::ws::{self, Config, Message};
use quench_proto::{Request, Response, StatusCode};

async fn socket(req: Request) -> Response {
    let handler = |mut sock: ws::WebSocket<quench_proto::io::Upgraded>| async move {
        while let Ok(Some(msg)) = sock.recv().await {
            if let Message::Text(t) = msg { let _ = sock.send(Message::Text(t)).await; } // echo
        }
    };
    match ws::accept(&req.method, &req.headers, Config::default(), handler) {
        Ok(response) => response,                       // 101 Switching Protocols
        Err(_) => Response::new(StatusCode::BAD_REQUEST),
    }
}
```

`recv()` answers pings itself, reassembles fragments and returns the peer's close once (`Message::Close`), after which it yields `Ok(None)`. A frame that breaks the rules makes the connection send a close with code `1002`, `1007` or `1009` and return `Err`.
