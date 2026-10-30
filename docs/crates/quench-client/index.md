---
title: quench-client
parent: Crates
nav_order: 9
has_children: true
permalink: /crates/quench-client/
---

`quench-client` is the HTTP/HTTPS client of the workspace. It has two layers: `transport`, a small request/response client (keep-alive pool, redirects, JSON/form/multipart bodies, cookies, streaming) built on `quench-proto`, and thin typed wrappers at the crate root (`HttpClient`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient`) that add authentication and JSON decoding for service-to-service calls. It replaces `reqwest`.

| | |
|---|---|
| Crate / library name | `quench-client` / `quench_client` |
| Version | 0.2.17 |
| Edition | 2024 (workspace) |
| Custom test harness | `harness = false` |
| Unsafe code | none |
| Used by | `quench-k8s`, `quench-auth`, `quench-tool`, `quench-starter` (all have it as a dependency) |

## Install

```toml
[dependencies]
quench-client = { version = "0.2", registry = "ennor" }
# synchronous programs:
# quench-client = { version = "0.2", registry = "ennor", features = ["blocking"] }
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `tls` | yes | `https://` through `quench-proto/tls` (rustls with the `ring` backend, Mozilla roots plus the OS CA bundle). Without it every `https` URL fails with `Error::Transport("cannot reach {host} over https: quench-client was built without its `tls` feature")` before any bytes are sent. `danger_accept_invalid_certs`, `add_root_certificate_pem` and `client_identity_pem` still exist but configure nothing. |
| `blocking` | no | adds `quench_client::blocking` (a synchronous client; enables `tokio/rt`). |

`quench-proto` is used with its `client` feature only.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-proto` (`client`; `tls` via the feature) | HTTP/1.1 codec and connection driver, `Uri`, `HeaderMap`, `Method`, `StatusCode`, TLS connector. |
| `quench-core` | `Result`, `Error`, `Context`, `err!`, `Bytes`, `base64`, `hex`, `random`, streams. |
| `quench-serde` | JSON and form encoding, `Serialize` / `DeserializeOwned`. |
| `quench-log` | `debug!` logging in `BasicAuthClient::get`. |
| `tokio` (`sync`, `time`, `net`) | sockets, timeouts, the token cache mutex; `rt` with `blocking`. |
| dev: `quench-test`, `quench-http` (`tls`), `tokio` (full set) | tests and in-process test servers. |

## Layout

| Module | Content | Page |
|---|---|---|
| crate root | `HttpClient`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient`, their builders, `ClientError`, `ClientResult`, `prelude` | [Typed clients](api-clients.md) |
| `transport` | `Client`, `ClientBuilder`, `RequestBuilder`, `Response`, `Redirect`, `Error`, `Result`, protocol re-exports | [Transport](api-transport.md) |
| `multipart` | `Form`, `Part` | [Multipart and blocking](api-multipart-blocking.md) |
| `blocking` (feature) | `Client`, `ClientBuilder`, `RequestBuilder`, `Response` | [Multipart and blocking](api-multipart-blocking.md) |

## Where to go next

* [Usage](usage.md)
* [Configuration](configuration.md): timeouts, TLS, redirects, size limits and pool constants.
* [Errors](errors.md)
* [Internals](internals.md): pooling, retry, redirects, token caching.
* [Testing](testing.md)
* [quench-tls](../quench-tls/index.md) for the TLS layer.

## Minimal example

```rust
use quench_client::transport::Client;
use quench_serde::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let response = client.get("https://example.org/api").send().await?;
    println!("{}", response.status());
    let value: Value = response.json().await?;
    println!("{value:?}");
    Ok(())
}
```

For typed service-to-service calls use the wrappers:

```rust
use quench_client::prelude::HttpClient;
use quench_serde::Value;

async fn run() -> quench_core::Result<()> {
    let api = HttpClient::new("http://localhost:8080")?;
    let health: Value = api.get("/health").await?;
    println!("{health:?}");
    Ok(())
}
```
