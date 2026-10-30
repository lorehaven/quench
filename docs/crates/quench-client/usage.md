---
title: Usage
parent: quench-client
grand_parent: Crates
nav_order: 1
---

Worked examples, from the simple typed clients to streaming. `Result` in these snippets is `quench_core::Result` unless a `transport` type is named.

## 1. Call a JSON API

```rust
use quench_client::prelude::HttpClient;
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize)] struct NewItem { name: String }
#[derive(Deserialize)] struct Item { id: u64, name: String }

async fn create(api: &HttpClient) -> quench_core::Result<Item> {
    api.post("/items", &NewItem { name: "a".into() }).await
}

fn make() -> quench_core::Result<HttpClient> {
    HttpClient::new("http://localhost:8080") // no trailing slash
}
```

Effect: `POST http://localhost:8080/items` with a JSON body; the 2xx JSON answer is decoded. A 4xx/5xx becomes `Err("HTTP 404 Not Found: <body>")`. Pitfalls: paths are concatenated to the base URL, so `/items` needs its slash; there is no timeout; HTTP/1.1 only.

## 2. Authenticated calls

```rust
use quench_client::prelude::{BasicAuthClient, BearerAuthClient};
use quench_serde::Value;

async fn run() -> quench_core::Result<()> {
    let basic = BasicAuthClient::builder("https://svc.internal")
        .username("admin")
        .password("secret")
        .build()?;
    let _: Value = basic.get("/api/v1/status").await?;
    basic.delete_expect_success("/ui/things/7").await?; // HTML or empty answer is fine

    let bearer = BearerAuthClient::new("https://svc.internal", "eyJ...")?;
    let _: Value = bearer.post("/api/v1/jobs", &quench_serde::json!({"run": true})).await?;
    Ok(())
}
```

`Debug` output of these clients and builders never prints the password or token.

## 3. Machine-to-machine with `client_credentials`

```rust
use quench_client::prelude::ClientCredentialsClient;
use quench_serde::Value;

async fn models(secret: &str) -> quench_core::Result<Value> {
    let client = ClientCredentialsClient::builder("https://switchboard.internal")
        .token_url("https://gatehouse.internal/api/v1/token")
        .client_id("sage")
        .client_secret(secret)
        .build()?;
    client.get("/api/v1/models").await
}
```

The first call exchanges the credentials for a token; later calls (and clones of the client) reuse it until 30 s before it expires. Keep one client for the life of the service so the cache works. Pitfall: if the target answers 401 because the token was revoked early, the call fails; the cache is only replaced when its time is up.

## 4. Anything the typed clients lack: `transport::Client`

```rust
use quench_client::transport::{Client, Method};
use std::time::Duration;

async fn patch_item(client: &Client) -> quench_client::transport::Result<()> {
    let response = client
        .request(Method::PATCH, "https://api.example.org/items/1")
        .bearer_auth("token")
        .timeout(Duration::from_secs(5))
        .json(&quench_serde::json!({"done": true}))
        .send()
        .await?;
    assert!(response.status().is_success());
    Ok(())
}
```

`transport::Client` does not look at the status code; check it yourself.

## 5. Upload a file

```rust
use quench_client::multipart::{Form, Part};
use quench_client::transport::Client;

async fn upload(client: &Client, bytes: Vec<u8>) -> quench_client::transport::Result<String> {
    let form = Form::new()
        .text("title", "scan")
        .part("file", Part::bytes(bytes).file_name("scan.png").mime_str("image/png"));
    client.post("http://localhost:9000/upload").multipart(form).send().await?.text().await
}
```

The whole body is built in memory; there is no streaming upload.

## 6. Cookies

```rust
# async fn run(client: quench_client::transport::Client) -> quench_client::transport::Result<()> {
let login = client
    .post("http://localhost:8080/login")
    .form(&[("user", "a"), ("password", "b")])
    .send()
    .await?;
let session = login.cookie("session").unwrap_or_default();
let page = client
    .get("http://localhost:8080/me")
    .cookies(&[("session", session.as_str())])
    .send()
    .await?;
# let _ = page; Ok(()) }
```

There is no cookie jar: you read `Set-Cookie` pairs and send them back yourself. The client never stores cookies between requests (including across redirects).

## 7. Read a stream (server-sent events, token streams)

See [Transport](api-transport.md#server-sent-events-and-large-downloads). `send_streaming` returns after the headers; the connection is dropped afterwards, not pooled.

## 8. Private CA and mutual TLS

```rust
use quench_client::transport::Client;

fn client(ca_pem: &[u8], cert_pem: &[u8], key_pem: &[u8]) -> quench_client::transport::Result<Client> {
    Client::builder()
        .add_root_certificate_pem(ca_pem)
        .client_identity_pem(cert_pem, key_pem)
        .build()
}
```

The CA is trusted in addition to the Mozilla roots and the operating-system bundle. `build()` fails with `Error::Build` when a PEM cannot be used.

## 9. Synchronous programs

With `features = ["blocking"]`:

```rust
use quench_client::blocking::Client;

fn main() -> Result<(), quench_client::transport::Error> {
    let client = Client::builder().user_agent("tool/1.0").build()?;
    let text = client.get("https://example.org/").send()?.text()?;
    println!("{}", text.len());
    Ok(())
}
```

Do not call it from within `#[tokio::main]` code.
