---
title: quench-proto
parent: Crates
nav_order: 35
has_children: true
permalink: /crates/quench-proto/
---

`quench-proto` (crate `quench_proto`) is the protocol layer under `quench-http` (server) and `quench-client`: the HTTP message types, a strict HTTP/1.1 codec with connection drivers for both ends, cookies, `multipart/form-data`, server-sent events, WebSocket (RFC 6455) and a re-export of the TLS glue. It replaces `http`, `http-body-util`, `hyper`, `hyper-util`, `multer`, `cookie` and the HTTP-facing role of `bytes`.

> **Not Protocol Buffers.** Despite its name this crate has nothing to do with protobuf; "proto" is the HTTP *protocol* layer. There is no protobuf code anywhere in it.

* Version: `0.1.10`. Edition 2024. No `unsafe` code (`unsafe_code = "deny"`; none present).
* **HTTP/1.1 only.** HTTP/2 and HTTP/3 are not implemented; `Version::HTTP_2` and `HTTP_3` exist as values only. TLS is normally terminated at the ingress; the `tls` feature re-exports `quench-tls` for services that terminate it themselves.
* Parsing is *strict*: anything that could make two parsers disagree about where a message ends is rejected (request-smuggling defence). See [Internals](internals.md).
* Async runtime: `tokio` (only the `io-util`, `net`, `sync`, `time` features). The drivers are generic over [`io::Io`](api.md#io).

## Install

```toml
[dependencies]
quench-proto = { version = "0.1", registry = "ennor" }                     # all default features
# quench-proto = { version = "0.1", registry = "ennor", default-features = false }   # types + codec only
```

## Feature flags

Default: `["server", "client", "ws", "multipart", "sse", "tls"]`.

| Feature | Adds | Implies / extra dependency |
|---|---|---|
| `server` | `h1::server` (connection driver) | none |
| `client` | `h1::client` (connection driver) | none |
| `ws` | `ws` (WebSocket) | `quench-crypto` (SHA-1 for the handshake: `hash::sha1_legacy`) |
| `multipart` | `multipart` | none |
| `sse` | `sse` | none |
| `tls` | `quench_proto::tls` = re-export of `quench-tls` | `quench-tls` (rustls + ring) |

With `default-features = false` you still get: the message types (`Method`, `StatusCode`, `Version`, `Uri`, headers, `Body`, `Request`, `Response`, `Extensions`), `cookie`, `date`, `error`, `io`, and the codec `h1::{parse, decode, reader}`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | `bytes::Bytes`, `stream::{Stream, StreamExt}`, `time::{Time, Duration}`, `random`, `base64` |
| `quench-crypto` (optional, `ws`) | SHA-1 for `Sec-WebSocket-Accept` |
| `quench-tls` (optional, `tls`) | TLS over rustls |
| `tokio` | `AsyncRead`/`AsyncWrite`, timers, channels |
| `quench-test`, `tokio` (macros, rt) (dev) | tests |

## Where to go next

* [Usage](usage.md): serving, calling, cookies, multipart, SSE, WebSocket.
* API reference: [core types](api.md), [HTTP/1.1 codec and drivers](api-h1.md), [cookie, multipart, sse, ws](api-extras.md).
* [Configuration](configuration.md): every limit, timeout and default.
* [Errors](errors.md)
* [Internals](internals.md): wire rules, smuggling defences, framing tables, WebSocket frame layout.
* [Testing](testing.md)

## Minimal example

```rust
use quench_proto::h1::server::{serve_connection, ServerConfig, Shutdown};
use quench_proto::{Request, Response, StatusCode};

async fn hello(_req: Request) -> Response {
    Response::new(StatusCode::OK).header("content-type", "text/plain").body("hello world")
}

# async fn run(tcp: tokio::net::TcpStream) -> Result<(), quench_proto::Error> {
// one connection: keep-alive, pipelining, limits and timeouts from ServerConfig::default()
serve_connection(tcp, &hello, &ServerConfig::default(), Shutdown::never()).await
# }
```
