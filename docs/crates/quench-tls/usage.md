---
title: Usage
parent: quench-tls
grand_parent: Crates
nav_order: 1
---

## 1. A TLS server over TCP

```rust
use quench_tls::{TlsAcceptor, load_server_config};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = load_server_config("cert.pem", "key.pem").expect("usable certificate and key");
    let acceptor = TlsAcceptor::new(Arc::new(config));

    let listener = TcpListener::bind("127.0.0.1:8443").await?;
    loop {
        let (tcp, _) = listener.accept().await?;
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            let Ok(mut tls) = acceptor.accept(tcp).await else { return };
            let mut buf = [0u8; 1024];
            if let Ok(n) = tls.read(&mut buf).await {
                let _ = tls.write_all(&buf[..n]).await;
            }
            let _ = tls.shutdown().await; // sends close_notify
        });
    }
}
```

Effect: an echo server. Pitfalls: `load_server_config` returns `None` for any problem; to see the reason call `pem_certificates` / `pem_private_key` / `server_config` yourself. Call `shutdown()` before dropping, or a well-behaved client sees a truncated stream (`UnexpectedEof`).

## 2. Negotiate ALPN on the server

```rust
use quench_tls::{TlsAcceptor, load_server_config};

let mut config = load_server_config("cert.pem", "key.pem").unwrap();
config.alpn_protocols = vec![b"http/1.1".to_vec()];
let acceptor = TlsAcceptor::from(config);
// after accept(): tls.alpn_protocol() == Some(b"http/1.1".as_slice())
```

## 3. A verifying client with a private CA

```rust
use quench_tls::{ClientOptions, TlsConnector, client_config, server_name};

let ca = std::fs::read("internal-ca.pem").unwrap();
let config = client_config(&ClientOptions {
    use_system_roots: true,
    extra_roots_pem: vec![ca],
    alpn_protocols: vec![b"http/1.1".to_vec()],
    ..ClientOptions::default()
})?;
let connector = TlsConnector::from(config);
let name = server_name("db.internal")?;
```

Then `connector.connect(name, tcp).await`. The server name is verified against the certificate; an IP address works when the certificate lists it. Pitfall: `ClientOptions::default()` has `use_system_roots: false`.

## 4. Development against a self-signed server

```rust
let config = quench_tls::client_config(&quench_tls::ClientOptions {
    accept_invalid_certs: true,
    ..Default::default()
})?;
```

No certificate or host name is checked. Never ship this.

## 5. Mutual TLS from the client

```rust
use quench_tls::{ClientOptions, client_config_with_identity};

let cert = std::fs::read("client.pem")?;
let key = std::fs::read("client.key")?;
let config = client_config_with_identity(&ClientOptions::default(), &cert, &key)?;
```

A certificate chain that is empty, a key that does not match, or an unsupported key type give an `Err(String)` with a description.

## 6. Mutual TLS on the server

`server_config` does not request client certificates. Build the rustls config directly:

```rust
use quench_tls::{TlsAcceptor, pem_certificates, pem_private_key};
use rustls::crypto::ring::default_provider;
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use std::sync::Arc;

fn mutual_acceptor(ca_pem: &[u8], chain_pem: &[u8], key_pem: &[u8]) -> TlsAcceptor {
    let mut roots = RootCertStore::empty();
    for cert in pem_certificates(ca_pem).unwrap() {
        roots.add(cert).unwrap();
    }
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::new(default_provider()))
        .build()
        .unwrap();
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_client_cert_verifier(verifier)
        .with_single_cert(pem_certificates(chain_pem).unwrap(), pem_private_key(key_pem).unwrap())
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}
```

This is the `mutual_acceptor` helper of `tests/tls.rs`. It needs `rustls` 0.23 (features `ring`, `std`) as your own dependency; the providers are passed explicitly, so no default provider has to be installed.

## 7. Process-wide provider for third-party code

```rust
quench_tls::install_default_provider();
// now a library that calls rustls' `ClientConfig::builder()` finds ring
```
