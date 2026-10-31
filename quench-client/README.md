# quench-client
HTTP client abstraction with auth and retry support.

## What it is
`quench-client` is the HTTP/HTTPS client of the workspace and replaces `reqwest`. The `transport` layer is a small request/response client (keep-alive pool, redirects, JSON/form/multipart bodies, cookies, streaming) built on `quench-proto`. Thin typed wrappers at the crate root (`HttpClient`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient`) add authentication and JSON decoding for service-to-service calls.

It is used by `quench-k8s`, `quench-auth`, `quench-tool` and `quench-starter`.

## Use cases
- Call a JSON API with typed requests and responses.
- Authenticate with basic auth or a bearer token.
- Do machine-to-machine calls with the OAuth `client_credentials` flow.
- Use `transport::Client` directly for streaming or multipart requests.

## Example
```rust
use quench_client::prelude::{ClientCredentialsClient, HttpClient};
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize)] struct NewItem { name: String }
#[derive(Deserialize)] struct Item { id: u64, name: String }

async fn create(api: &HttpClient) -> quench_core::Result<Item> {
    api.post("/items", &NewItem { name: "a".into() }).await
}

fn make() -> quench_core::Result<HttpClient> {
    HttpClient::new("http://localhost:8080")
}

fn svc(secret: &str) -> quench_core::Result<ClientCredentialsClient> {
    ClientCredentialsClient::builder("https://switchboard.internal")
        .token_url("https://gatehouse.internal/api/v1/token")
        .client_id("sage")
        .client_secret(secret)
        .build()
}
```

## Features
- `tls` (default): `https://` support through rustls; without it every `https` URL fails with a transport error.
- `blocking`: adds `quench_client::blocking`, a synchronous client.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-client/
