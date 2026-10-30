---
title: Internals
parent: quench-client
grand_parent: Crates
nav_order: 7
---

## Connection pool

`transport::Client` holds `Mutex<HashMap<Origin, Vec<Idle>>>`, where `Origin = (secure, host, port)` (IPv6 brackets stripped). A connection is put back after the full body of a buffered response (or a drained redirect body) has been read and the driver reports it reusable. `take_idle` pops the newest idle connection that is younger than 60 s and still `is_reusable()`; on return the pool drops entries older than 60 s and keeps at most 8. Poisoned locks are recovered (`PoisonError::into_inner`).

`send_streaming` responses never go back to the pool.

## Retry on a dead connection

`round_trip` tries at most twice. Attempt 0 uses an idle connection if there is one. If that reused connection fails with `quench_proto::Error::Io` or `UnexpectedEof` and the method is idempotent, attempt 1 uses `take_idle` again (possibly another idle connection) or a fresh connection. Otherwise the error is returned as `Error::Transport`.

## Redirects

For `Redirect::Limited(n)` the loop runs up to `n + 1` times. A response is followed when its status is a redirection (3xx) other than `304`, and it has a `Location` header. The target is resolved against the current URL (`Uri::resolve`, so relative targets work). The body is read up to 64 KiB to recycle the connection. `301`, `302` and `303` change a non-GET/HEAD method to `GET`, drop the body and remove `Content-Type`; `307` and `308` replay the method and body. All other request headers are kept, except that `Authorization`, `Cookie` and `Proxy-Authorization` are removed when the target is another origin (another scheme, host or port), so credentials never follow a redirect to a different site. After `n` hops: `Error::Transport("too many redirects")`. With `Redirect::None` the 3xx response is returned as is.

## Response bodies

`send` collects the body with `collect_limited(max_response_bytes or usize::MAX)`; exceeding the limit is `Error::Transport`. The `Response` then holds `Bytes`. `send_streaming` keeps the `quench_proto::Body` and hands it out through `bytes_stream`, or collects it later with the same limit.

## Token cache

`ClientCredentialsClient` stores `Option<CachedToken { access_token, expires_at }>` in an `Arc<tokio::sync::Mutex<..>>`. The lock is held only while reading or writing the slot, **not during the token request**: concurrent callers that find the cache empty each perform an exchange, and the last writer wins. `CachedToken`'s `Debug` redacts the token. The client holds two `transport::Client` handles that are clones of one pool.

## HTTP version

Only HTTP/1.1 (`quench_proto::h1`). ALPN offers `http/1.1` only. No HTTP/2, no proxies (no `HTTP_PROXY` handling), no compression: no `Accept-Encoding` is sent and a compressed body is not decoded.

## Thread-safety

`transport::Client`, the typed clients and their builders are `Clone + Send + Sync`. A `RequestBuilder` is `Send`. The blocking client holds an `Arc<Runtime>`; since the runtime is current-thread, calls from several threads on clones of one client are serialised by tokio's `block_on` (not verified by a test).
