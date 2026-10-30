---
title: Testing
parent: quench-proto
nav_order: 8
---

* Unit tests sit in each module (`#[quench_test::test]`; `lib.rs` has `quench_test::main!()`).
* Integration tests (`harness = false`; the first two need features `client` and `server`):
  * `tests/h1_server.rs`: raw wire bytes over in-memory pipes (pipelining, chunking, `Expect: 100-continue`, HEAD, smuggling vectors, limits, timeouts, body drain, upgrade, shutdown).
  * `tests/h1_client.rs`: scripted servers and the server driver against the client.
  * `tests/security_h1.rs`: adversarial cases (listed by name in the source): `Content-Length` with `Transfer-Encoding`, conflicting and malformed lengths, obfuscated `Transfer-Encoding`, bare LF, obs-fold, control characters, malformed request lines, hostile chunk sizes, pipelined requests after a bad one, oversized heads, too many headers, header floods, slowloris, idle reaping, clients that never read, huge declared bodies, CRLF in response header values. Each asserts what the client was told and how many times the handler ran.
  * `tests/fuzz_parsers.rs`: dependency-free mutation fuzzing of request/response heads, framing, the chunked decoder (split-independence), WebSocket frames, URIs, cookies, multipart boundaries. Deterministic per seed; environment variables `QUENCH_FUZZ_ITERS` (default 20000 per target) and `QUENCH_FUZZ_SEED` tune it. A failure prints the seed and input.
  * `tests/throughput.rs`: an `#[ignore]`d smoke test for keep-alive requests per second: `cargo test --release -p quench-proto --test throughput -- --ignored --nocapture`.
* Run: `cargo test -p quench-proto` (all default features).
* Testing code built on it: `serve_connection` and `send_request` work on any `Io`, e.g. `tokio::io::duplex`, so no sockets are needed; `Shutdown::never()` for connections that should not be stopped.
