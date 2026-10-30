---
title: Testing
parent: quench-client
grand_parent: Crates
nav_order: 8
---

## How the crate is tested

| Target | File | Covers |
|---|---|---|
| `unit` | `tests/unit/lib_tests.rs`, `tests/unit/mock_server.rs` | the four typed clients against a raw-socket test double (`serve_once`, `serve_n`): methods, JSON, error statuses, auth headers, `delete_expect_success`, token exchange and caching, refused/unparsable token responses, secrets absent from `Debug`, builder validation |
| `transport` | `tests/transport.rs` | JSON/form/basic/bearer, redirects (methods, policy `None`, loops, relative targets, connection reuse), multipart, cookies, timeouts and refused connections, https refused without `tls`, self-signed rejection, private CA, mutual TLS identity, connection reuse and dead-connection retry, no replay of non-idempotent requests, size limits, bad URLs, IPv6 literals, `User-Agent`, streaming, bulk headers, `PATCH`/`HEAD`, query encoding. Uses an in-process `quench-http` server (dev-dependency with `tls`) |
| `blocking` | `tests/blocking.rs` (needs `--features blocking`) | blocking calls, redirects, cookies, multipart |
| in-crate | `src/multipart.rs` | body encoding, boundaries, escaping |

Fixtures: `tests/fixtures/localhost.pem` / `.key` (server) and `client.pem` / `.key` (client identity), test-only material.

```bash
cargo test -p quench-client
cargo test -p quench-client --features blocking
cargo test -p quench-client --no-default-features   # the `https refused` test
```

No test reaches the internet.

## Testing your own code

Start a local server and point the client at its address. `quench-http` can serve in-process (see `tests/transport.rs` for `plain_server()`), or use a raw `tokio::net::TcpListener` as `tests/unit/mock_server.rs` does. Build the client with `HttpClient::new(&format!("http://{addr}"))`; plain HTTP needs no certificate set-up.
