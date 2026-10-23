# Quench Proto

`quench-proto` (crate `quench_proto`) is the protocol layer under [Quench Http](./quench-http.md) and [Quench Client](./quench-client.md). It replaces `http`, `http-body-util`, `hyper`, `hyper-util`, `multer`, `cookie` and `bytes`' role in the HTTP stack: message types, an HTTP/1.1 codec with connection drivers for both ends, multipart, cookies, WebSocket and server-sent events. The only dependencies are `quench-core`, `quench-crypto` (SHA-1 for the WebSocket handshake), [`quench-tls`](./quench-tls.md) and `tokio`; the drivers, WebSocket, multipart, SSE and TLS are [cargo features](#features).

**HTTP/2 is not implemented.** TLS is terminated at the ingress in every deployment, so h2 would only add a second protocol to maintain and secure. The drivers are generic over [`io::Io`] (any `AsyncRead + AsyncWrite` stream), and `h1` is a module like any other, so another protocol version can be added later next to it without touching the callers.

## Features

All on by default; `default-features = false` keeps only the types, the cookie and date helpers and the HTTP/1.1 codec (`h1::{parse, decode, reader}`).

| Feature | Adds | Extra dependencies |
| --- | --- | --- |
| `server` | `h1::server`, the server connection driver | none |
| `client` | `h1::client`, the client connection driver | none |
| `ws` | `ws`, WebSocket (RFC 6455) | `quench-crypto` (SHA-1 for the handshake) |
| `multipart` | `multipart` | none |
| `sse` | `sse`, server-sent events | none |
| `tls` | `quench_proto::tls`, a re-export of [`quench-tls`](./quench-tls.md) | `rustls`, `ring`, `webpki-roots` |

With nothing enabled the crate pulls in 15 third-party crates (tokio and the proc-macro stack); the defaults pull in 25.

## Types

`Method`, `StatusCode`, `Version`, `Uri`, `HeaderName`/`HeaderValue`/`HeaderMap`, `Extensions`, `Body`, `Request` and `Response` — shaped like the `http` crate's, with these differences:

- `HeaderMap` is an ordered multimap with linear lookups (requests are capped at 100 headers, where a scan beats hashing). Names are case-insensitive; `get("Content-Type")` works. `get_all`, `append` (for `Set-Cookie`), `insert` (replaces all values), `remove`, `keys`, `get_joined` (a list header sent over several lines, joined with `, `). The well-known names are constants in `header::` (`CONTENT_TYPE`, `AUTHORIZATION`, ...).
- `Uri` accessors return `&str` (`scheme()`, `authority()`, `host()`, `path()`, `query()`, `path_and_query()`) and `port_u16()`. It parses origin form, absolute form, authority form and `*`; a fragment is rejected, and an absolute URL with no path gets `/`. `resolve(location)` follows a redirect target.
- `Body` is a stream of `Bytes` chunks (`quench_core::stream::Stream`) with an optional exact length: fixed bodies (`Body::from_bytes`) are sent with `Content-Length`, `Body::from_stream` is sent chunked, `Body::from_stream_sized` declares a length for a streamed body. `collect_limited(n)` buffers up to `n` bytes.
- `Response::on_upgrade` hands the raw connection to a callback after the head is written (used by WebSocket).

## HTTP/1.1 (`h1`)

- **`parse`** — strict head parsing with `Limits` (head 64 KiB, 100 headers, 8 KiB target). Rejected rather than interpreted, because two parsers disagreeing about where a message ends is how request smuggling works: obsolete line folding, whitespace before the colon, a bare LF, NUL/CR inside a line, conflicting or malformed `Content-Length` (lists, signs, overflow), both `Content-Length` and `Transfer-Encoding`, a `Transfer-Encoding` that is not exactly `chunked`, a missing or duplicate `Host` on HTTP/1.1. Each failure maps to a status (`400`, `414`, `431`, `501`, `505`).
- **`decode`** — incremental body decoders: fixed length, chunked (hex size, extensions ignored, trailers read and discarded within limits) and read-to-EOF. They never consume past the end of the message, so pipelined requests work.
- **`server::serve_connection(io, &service, &config, shutdown)`** — reads a request, runs the `Service`, writes the response, repeats while the connection is reusable. Keep-alive and pipelining, `Expect: 100-continue`, `HEAD`, a request body streamed to the handler through a bounded channel (backpressure), and a response that may stream the request body back. A handler that ignores its body does not wedge the connection: the rest is discarded up to `drain_limit`, otherwise the response carries `Connection: close`. Timeouts: header read (slowloris), idle between requests, per write. `shutdown_channel()` gives a handle whose signal makes connections finish the request in flight and stop; idle ones exit at once.
- **`client::send_request(conn, request, &config)`** — one request on a `ClientConn`; the `Exchange` holds the response (body streaming) and a `reuse` receiver that yields the connection back once the body was read to its end and the peer allows reuse. `ClientConn::is_reusable()` tells whether an idle connection was closed under you. Interim `1xx` responses are skipped.

## Other modules

- **`cookie`** — RFC 6265. `Cookie::parse`, `Cookie::build((name, value)).path(..).secure(..).http_only(..).same_site(..).max_age(..).build()`, `Display` as a `Set-Cookie` value (`HttpOnly; SameSite; Secure; Path; Domain; Max-Age; Expires`, the `cookie` crate's order), and `cookie_value(header, name)` for the request side. A leading dot on `Domain` is ignored.
- **`multipart`** — a streaming `multipart/form-data` reader over any chunk stream: `parse_boundary`, `Multipart::next_field()`, `Field::chunk()/bytes()/text()`. Only a delimiter-sized tail is held back while looking for the next boundary. Limits for the whole stream, per field, header block and field count; RFC 5987 `filename*`, quoted names, preamble/epilogue and transport padding handled.
- **`ws`** — RFC 6455. The handshake (`accept_key`, `validate_upgrade`, `accept` building the `101` response), frame `encode_frame`/`decode_frame`, and `WebSocket<Io>` with `recv()`/`send()`/`close()`: fragmentation, auto-pong, the closing handshake, UTF-8 and close-code validation, size limits (close `1009`), no extensions. Frames that break the rules close the connection with `1002`/`1007`/`1009`.
- **`sse`** — `Event` builder (`data`, `event`, `id`, `retry`, `comment`) and `response(stream)`; newlines cannot escape a single-line field.
- **`date`** — IMF-fixdate formatting and parsing.
- **`tls`** — re-export of [`quench-tls`](./quench-tls.md).

## Verification

Unit tests per module, plus `tests/h1_server.rs` (raw wire bytes over in-memory pipes: pipelining, chunking, smuggling vectors, limits, timeouts, drain, upgrade, shutdown) and `tests/h1_client.rs` (scripted servers and the server driver). The framework-level tests in `quench-http` and `quench-client` run the whole stack over real TCP and TLS.

[Home](../README.md)
