# Quench Client

`quench-client` (crate `quench_client`) is Forge's shared HTTP client abstraction: thin wrappers over its own small client built on [Quench Proto](./quench-proto.md) (`quench_client::transport`: keep-alive pooling with dead-connection detection and one retry for idempotent requests, HTTP/1.1 only, JSON/form bodies, basic/bearer auth, timeouts, redirects, Mozilla roots plus the OS CA bundle and `add_root_certificate_pem` for a private CA) that add authentication (Basic, Bearer, OAuth2 `client_credentials`) and consistent JSON error handling, so services calling each other over HTTP don't each reimplement auth headers and response parsing. This page is the full reference (based on the single source file `libs/quench-client/src/lib.rs`); the crate's own README is a short pointer here. sage-service depends on it — `docker/sage-service/src/clients/switchboard.rs` uses `ClientCredentialsClient` for machine-to-machine calls to switchboard — and `cli/welder` uses `BearerAuthClient` in its switchboard model client.

## Features

`tls` (default) enables `https://` and the certificate settings that go with it. Without it the client is built on the plain HTTP/1.1 driver alone, and a request to an `https` URL fails with `Error::Transport` ("built without its `tls` feature") before any bytes are sent. `danger_accept_invalid_certs` and `add_root_certificate_pem` still exist so calling code compiles unchanged, but they have nothing to configure. The client depends on `quench-proto`'s `client` feature only, never on the server side.

`blocking` (off by default) adds `quench_client::blocking`: the same `Client` / `RequestBuilder` / `Response` for synchronous programs, each call blocking until the response is in. A blocking `Client` owns a small private tokio runtime that drives its connections (so the pool is reused between calls) and, like every blocking HTTP client, must not be used from inside an async runtime.

## Public API / Key Types

`quench_client::transport` is the plain client the wrappers below are built on: `Client::builder()` (`timeout`, `danger_accept_invalid_certs`, `add_root_certificate_pem`, `max_response_bytes`, `user_agent`, `client_identity_pem(cert, key)` (a client certificate for mutual TLS), `redirect(Redirect::None | Redirect::Limited(n))` - follows 10 by default; `None` hands the 3xx response and its `Location` to the caller), `get`/`post`/`put`/`patch`/`delete`/`head`/`request`, and on the request `header`, `headers(HeaderMap)`, `query(&[(k, v)])` (percent-encoded), `basic_auth`, `bearer_auth`, `json`, `form`, `body`, `timeout`, `multipart(Form)` (`quench_client::multipart`: `Form::new().text("k", "v").part("file", Part::bytes(..).file_name(..).mime_str(..))`, random boundary, quotes and line breaks in names escaped), `cookies(&[(name, value)])` (one `Cookie` header), then `send()` (the whole body, buffered) or `send_streaming()` (returns once the headers are in; read the body with `Response::bytes_stream()`, or `bytes`/`text`/`json`, and the connection is not reused). `Response::cookies()` / `cookie(name)` read the `name=value` pairs of its `Set-Cookie` headers (attributes dropped); the blocking client has all of the above too. It re-exports `HeaderMap`, `HeaderName`, `HeaderValue`, `Method`, `StatusCode`, `Uri` and the `header` constants, so a caller needs no `quench-proto` dependency for them. `user_agent` is sent on every request (GitHub's API refuses requests without one); a request can still set its own.

Re-exported from `quench_client::prelude`: `HttpClient`, `BasicAuthClient`, `BearerAuthClient`, `ClientCredentialsClient`.

- **`HttpClient`** — unauthenticated base client. `HttpClient::new(base_url)` or `HttpClient::builder(base_url).tls_verify(bool).build()`. `get`/`post`/`put`/`delete`, each generic over a `DeserializeOwned` response type (and `Serialize` body for `post`/`put`). Non-2xx responses become an `anyhow::Error` carrying the status and body text.
- **`BasicAuthClient`** — wraps an `HttpClient`, sending HTTP Basic auth on every call. `BasicAuthClient::new(base_url, username, password)` or `.builder(base_url).username(..).password(..).tls_verify(..).build()`. Same `get`/`post`/`put`/`delete`, plus `delete_expect_success(path)` for endpoints returning HTML or empty bodies that can't be deserialized (treats any 2xx as success and discards the body).
- **`BearerAuthClient`** — wraps an `HttpClient` with a fixed bearer token. `BearerAuthClient::new(base_url, token)` or `with_tls_verify(base_url, token, verify)`. `get`/`post`.
- **`ClientCredentialsClient`** — authenticates via the OAuth2 `client_credentials` grant against gatehouse's token endpoint instead of sending a shared secret to the target service directly. Built with `.builder(base_url).token_url(..).client_id(..).client_secret(..).tls_verify(..).build()`. Exchanges once, caches the access token in an internal `Mutex`, and re-exchanges automatically ~30s before expiry. `get`/`post`/`delete_expect_success`.
- **`ClientError`** / **`ClientResult<T>`** — `HttpError`, `RequestFailed`, `ParseError`, `ConnectionError` variants (defined but the client methods currently surface errors as `anyhow::Error`, not these variants).

## Configuration

No environment variables are read directly by this crate — `base_url`, `token_url`, credentials, and TLS verification are all supplied by the caller at construction time (e.g. sage-service reads `GATEHOUSE_URL`/client id/secret itself and passes them into `ClientCredentialsClient::builder`).

## Usage example

```rust
use quench_client::prelude::ClientCredentialsClient;

let client = ClientCredentialsClient::builder("https://switchboard.internal")
    .token_url("https://gatehouse.internal/api/v1/token")
    .client_id("sage")
    .client_secret(&client_secret)
    .build()?;

let models: ModelsResponse = client.get("/api/v1/models").await?;
```

[Home](../README.md)
