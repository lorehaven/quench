---
title: Typed clients
parent: quench-client
grand_parent: Crates
nav_order: 2
---

Reference for the crate root (source: `src/lib.rs`). All four clients call a path appended to a **base URL** by plain string concatenation (`format!("{base_url}{path}")`): give the path a leading `/` and the base no trailing `/`. `Result` below is `quench_core::Result`.

Every request/response method works the same way:

* The body (`post`, `put`) is serialised as JSON with `Content-Type: application/json`.
* A non-2xx status gives `Err` with the message `HTTP {status}: {body}` (`{status}` is the `StatusCode` display, e.g. `404 Not Found`; the body is the response text, or `Unknown error` when it cannot be read).
* A 2xx response is decoded as JSON into `T`; a decode failure gives `Err` with the context `Failed to parse response`.
* A send failure gives `Err` with the context `Failed to send GET request` (`POST`, `PUT`, `DELETE` respectively) and the transport error as its cause.
* Redirects are followed (up to 10, see [Transport](api-transport.md#redirect)). No timeout is set: a hung server hangs the call.

## `ClientError` and `ClientResult`

```rust
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]       HttpError(String),
    #[error("Request failed: {0}")]   RequestFailed(String),
    #[error("Parse error: {0}")]      ParseError(String),
    #[error("Connection error: {0}")] ConnectionError(String),
}
pub type ClientResult<T> = std::result::Result<T, ClientError>;
```

`Error` is `quench_core::Error` (derive macro). **No function in the crate returns `ClientError` or `ClientResult`**: they are public but unused. The clients return `quench_core::Result`.

## `HttpClient`

```rust
#[derive(Debug, Clone)]
pub struct HttpClient { /* http: transport::Client, base_url: String */ }
```

Unauthenticated JSON client. Cheap to clone (clones share the pool).

| Method | Signature |
|---|---|
| `new` | `pub fn new(base_url: &str) -> Result<Self>` (`builder(base_url).build()`, TLS verification on) |
| `builder` | `#[must_use] pub fn builder(base_url: &str) -> HttpClientBuilder` |
| `get` | `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>` |
| `post` | `pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |
| `put` | `pub async fn put<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |
| `delete` | `pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T>` |

### `HttpClientBuilder`

```rust
#[derive(Debug)]
pub struct HttpClientBuilder { /* base_url, tls_verify */ }
```

| Method | Signature | Default |
|---|---|---|
| `tls_verify` | `#[must_use] pub const fn tls_verify(self, verify: bool) -> Self` | `true` |
| `build` | `pub fn build(self) -> Result<HttpClient>` | fails with the `transport::Error::Build` text if the TLS config cannot be built |

`tls_verify(false)` calls `danger_accept_invalid_certs(true)`: no certificate or host-name check at all. Development only.

## `BasicAuthClient`

```rust
#[derive(Clone)]
pub struct BasicAuthClient { /* client: HttpClient, username, password */ }
```

Adds `Authorization: Basic base64(user:password)` to every request. `Debug` prints the password as `<redacted>`.

| Method | Signature |
|---|---|
| `new` | `pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self>` |
| `builder` | `#[must_use] pub fn builder(base_url: &str) -> BasicAuthClientBuilder` |
| `get` | `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>` (also logs the URL and username at `debug` level, never the password) |
| `post` | `pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |
| `put` | `pub async fn put<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |
| `delete` | `pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T>` |
| `delete_expect_success` | `pub async fn delete_expect_success(&self, path: &str) -> Result<()>`: any 2xx is success and the body is discarded; non-2xx gives `HTTP {status}: {body}`. For endpoints that answer with HTML or nothing. |

### `BasicAuthClientBuilder`

```rust
pub struct BasicAuthClientBuilder { /* base_url, tls_verify, username, password */ }
```

`Debug` is implemented by hand and redacts the password.

| Method | Signature | Default |
|---|---|---|
| `username` | `#[must_use] pub fn username(self, username: &str) -> Self` | `""` |
| `password` | `#[must_use] pub fn password(self, password: &str) -> Self` | `""` |
| `tls_verify` | `#[must_use] pub const fn tls_verify(self, verify: bool) -> Self` | `true` |
| `build` | `pub fn build(self) -> Result<BasicAuthClient>` | |

Credentials are not required: a builder without `username`/`password` sends `Basic Og==` (empty user and password).

## `BearerAuthClient`

```rust
#[derive(Clone)]
pub struct BearerAuthClient { /* client: HttpClient, token */ }
```

Sends `Authorization: Bearer {token}` with a fixed token. `Debug` redacts the token. Only `get` and `post` exist.

| Method | Signature |
|---|---|
| `new` | `pub fn new(base_url: &str, token: &str) -> Result<Self>` (TLS verification on) |
| `with_tls_verify` | `pub fn with_tls_verify(base_url: &str, token: &str, tls_verify: bool) -> Result<Self>` |
| `get` | `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>` |
| `post` | `pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |

There is no builder, `put` or `delete`; use `transport::Client` with `.bearer_auth(token)` for those.

## `ClientCredentialsClient`

```rust
#[derive(Clone)]
pub struct ClientCredentialsClient { /* client, http, token_url, client_id, client_secret, cached: Arc<tokio::sync::Mutex<Option<CachedToken>>> */ }
```

Authenticates with the OAuth2 `client_credentials` grant against a token endpoint (gatehouse), then calls the target service with `Authorization: Bearer {access_token}`; the target never sees the secret. `Debug` redacts the secret. Clones share the token cache.

| Method | Signature |
|---|---|
| `builder` | `#[must_use] pub fn builder(base_url: &str) -> ClientCredentialsClientBuilder` |
| `get` | `pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T>` |
| `post` | `pub async fn post<S: Serialize + Sync, T: DeserializeOwned>(&self, path: &str, body: &S) -> Result<T>` |
| `delete_expect_success` | `pub async fn delete_expect_success(&self, path: &str) -> Result<()>` |

No `put` or typed `delete`. There is no `new`.

Token exchange (private `access_token` / `refresh_token`):

1. Before each call, if a cached token exists and `expires_at > now`, it is used.
2. Otherwise `POST {token_url}` with the form body `grant_type=client_credentials&client_id=..&client_secret=..` (`application/x-www-form-urlencoded`), without `Authorization`.
3. A non-2xx answer gives `Err("client_credentials grant was refused: {body}")`. A transport failure gives the context `client_credentials token request failed`. An unparsable 2xx body gives `failed to parse the client_credentials token response`. The expected response is JSON with `access_token: String` and `expires_in: i64` (seconds); other fields are ignored.
4. The token is cached for `max(expires_in, 30) - 30` seconds, that is, it is refreshed 30 s before expiry; with `expires_in <= 30` the cache lifetime is 0 and every call exchanges again.

A 401 from the target is **not** retried with a new token; the error is `HTTP 401 ...` and the cache is kept until its time is up.

### `ClientCredentialsClientBuilder`

```rust
pub struct ClientCredentialsClientBuilder { /* base_url, token_url, client_id, client_secret, tls_verify */ }
```

`Debug` is implemented by hand and redacts the secret.

| Method | Signature | Default |
|---|---|---|
| `token_url` | `#[must_use] pub fn token_url(self, url: &str) -> Self` | `""` (a request then fails with an invalid-URL error) |
| `client_id` | `#[must_use] pub fn client_id(self, id: &str) -> Self` | `""` |
| `client_secret` | `#[must_use] pub fn client_secret(self, secret: &str) -> Self` | `""` |
| `tls_verify` | `#[must_use] pub const fn tls_verify(self, verify: bool) -> Self` | `true` |
| `build` | `pub fn build(self) -> Result<ClientCredentialsClient>` | |

`tls_verify` applies to both the token request and the API calls (one shared `transport::Client`).

## `prelude`

```rust
pub mod prelude {
    pub use crate::{BasicAuthClient, BearerAuthClient, ClientCredentialsClient, HttpClient};
}
```

The builders, `ClientError` and `ClientResult` are not in the prelude.
