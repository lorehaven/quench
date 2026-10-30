---
title: quench-tls
parent: Crates
nav_order: 53
has_children: true
permalink: /crates/quench-tls/
---

`quench-tls` is the thin layer between tokio byte streams and [rustls](https://github.com/rustls/rustls) that the workspace's network crates share: `TlsAcceptor` and `TlsConnector` run a handshake over any `AsyncRead + AsyncWrite` and give back a `TlsStream`, and a few helpers build client and server configurations and trust stores. It replaces `tokio-rustls`, `hyper-rustls`, `rustls-platform-verifier` and per-crate copies of root-store set-up.

| | |
|---|---|
| Crate / library name | `quench-tls` / `quench_tls` |
| Version | 0.1.8 |
| Edition | 2024 (workspace) |
| Crypto backend | `ring` (rustls features: `ring`, `std`, `tls12`, `logging`; `aws-lc-rs` is not built) |
| Protocol versions | TLS 1.2 and 1.3 (rustls safe defaults) |
| Unsafe code | none |
| Used by | `quench-proto` (re-exported as `quench_proto::tls`), `quench-pg`, `quench-redis`, `quench-mail` |

## Install

```toml
[dependencies]
quench-tls = { version = "0.1", registry = "ennor" }
```

## Feature flags

None. The crate declares no `[features]`; the `tls` features of its users (`quench-client`, `quench-proto`, `quench-pg`, `quench-redis`) decide whether it is built.

## Dependencies

| Dependency | Why |
|---|---|
| `rustls` 0.23 (`ring`, `std`, `tls12`, `logging`; no default features) | the TLS state machine |
| `webpki-roots` 1.0 | the Mozilla root certificates |
| `tokio` (`io-util`) | `AsyncRead` / `AsyncWrite` / `ReadBuf` |
| `quench-core` | `quench_core::env::var` to read `SSL_CERT_FILE` |
| dev: `quench-test`, `tokio` (`macros`, `rt`, `rt-multi-thread`) | tests |

## Where to go next

* [Usage](usage.md): server and client examples.
* [API](api.md): every public item.
* [Configuration](configuration.md): trust-store sources, `SSL_CERT_FILE`, options.
* [Errors](errors.md)
* [Internals](internals.md): how the stream adapts rustls to tokio, shutdown and EOF semantics.
* [Testing](testing.md)

## Minimal example

```rust
use quench_tls::{ClientOptions, TlsConnector, client_config, server_name};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = client_config(&ClientOptions {
        use_system_roots: true,
        ..ClientOptions::default()
    })?;
    let connector = TlsConnector::new(config);

    let tcp = TcpStream::connect(("example.org", 443)).await?;
    let mut tls = connector.connect(server_name("example.org")?, tcp).await?;

    tls.write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\nConnection: close\r\n\r\n").await?;
    let mut response = Vec::new();
    tls.read_to_end(&mut response).await?;
    println!("{} bytes", response.len());
    Ok(())
}
```
