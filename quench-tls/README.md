# quench-tls
A thin async glue between tokio streams and rustls, with the trust-store helpers quench's servers and clients share.

## What it is
`quench-tls` is the thin layer between tokio byte streams and rustls that the workspace's network crates share. `TlsAcceptor` and `TlsConnector` run a handshake over any `AsyncRead + AsyncWrite` and give back a `TlsStream`, and a few helpers build client and server configurations and trust stores.

It replaces `tokio-rustls`, `hyper-rustls`, `rustls-platform-verifier` and per-crate copies of root-store set-up. It is used by `quench-proto`, `quench-pg`, `quench-redis` and `quench-mail`.

## Use cases
- Terminate TLS on a TCP server from a certificate and key in PEM files.
- Connect to a TLS server with system roots, or with a private CA added.
- Negotiate ALPN protocols on a server or client.
- Present a client certificate for mutual TLS.
- Skip verification against a self-signed server during development.

## Example
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

Server side: `TlsAcceptor::new(Arc::new(load_server_config("cert.pem", "key.pem").unwrap()))`, then `acceptor.accept(tcp).await`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-tls/
