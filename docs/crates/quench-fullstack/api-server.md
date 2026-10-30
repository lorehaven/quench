---
title: API - server and testing
parent: quench-fullstack
grand_parent: Crates
nav_order: 4
---

Everything here is `not(target_arch = "wasm32")`.

## `server` (`src/server.rs`)

The server half of server functions: the registry `#[server]` fills, the checks every call passes, and the mapping of errors to responses.

```rust
#[derive(Debug)]
pub struct ServerFn {
    pub path: &'static str,
    pub endpoint: fn() -> Arc<dyn Endpoint>,
}

pub static FUNCTIONS: Registry<ServerFn>;      // quench_core::registry::Registry

#[must_use] pub fn functions() -> Vec<(&'static str, Arc<dyn Endpoint>)>;
#[must_use] pub fn collect<'a>(items: impl IntoIterator<Item = &'a ServerFn>) -> Vec<(&'static str, Arc<dyn Endpoint>)>;
#[must_use] pub fn guard(req: &Request) -> Option<Response>;
pub async fn dispatch<A, T, Fut>(req: Request, call: impl FnOnce(A) -> Fut) -> Response
    where A: Deserialize, T: Serialize, Fut: Future<Output = Result<T, ServerError>>;
pub async fn with_user<T>(user: Option<User>, future: impl Future<Output = T>) -> T;

impl ServerError {
    #[must_use] pub fn from_http(error: &HttpError) -> Self;
    #[must_use] pub fn into_response(self) -> Response;
}
impl User { #[must_use] pub fn from_claims(claims: &Claims) -> Self; }
impl FromRequest for User {}
```

| Item | Behaviour |
|---|---|
| `ServerFn` | One registered function: its route and a constructor of its endpoint. |
| `FUNCTIONS` | Every `#[server]` in the linked binary registers itself here before `main` (`submit!`). As for routes, a crate whose code the binary never references contributes nothing. |
| `functions()` | The registered functions as `(path, endpoint)` in registration order (the registry iterates newest first; the result is reversed). **Panics** if two functions have the same path: `two server functions share the path {path}; rename one or give it #[server(path = "...")]`. |
| `collect(items)` | The same for any iterator, in the order given; same panic. |
| `guard(req)` | Checks run before anything else. Returns `Some(response)` to send instead, else `None`. See the check order below. |
| `dispatch(req, call)` | After `guard` and user lookup: reads the body (at most `MAX_BODY`), decodes the arguments `A` from the JSON object, awaits `call(args)`, and answers `200` JSON with `cache-control: no-store`, or the error's response. An oversized body or read failure goes through `ServerError::from_http(&HttpError::from(e))`; undecodable arguments are `Validation { message: "invalid arguments: {e}", fields: [] }` (422); a result that cannot be serialized is `Internal`. |
| `with_user(user, future)` | Runs `future` with `user` as the answer of `crate::user()` (a `tokio::task_local!`). |
| `ServerError::from_http` | `HttpError::Status(code, msg)`: 401 `Unauthorized`, 403 `Forbidden`, 404 `NotFound`, 400/413/415/422 `Validation` (no fields), other codes `Internal`. `HttpError::Extraction(e)`: `Validation { message: e.to_string(), fields: [] }`. |
| `ServerError::into_response` | Logs an `Internal` cause with `quench_log::error!("server function failed: {cause}")`; answers with `self.status()` and `self.to_body()` as JSON and `cache-control: no-store`. (A body that cannot be serialized: 500 text `internal server error`.) |
| `User::from_claims` | `sub`, `scope`, `audiences = claims.aud`, `session = claims.sid`. |
| `impl FromRequest for User` | In order: the `Claims` already in the request extensions (set by `quench-auth`'s `Auth` middleware); else, using the `JwtConfig` from the container (missing: `HttpError` 500 `authentication is not configured`), a bearer token (`Authorization: Bearer ..`) that decodes, is for this service (`claims.allows(service_name)`) and, if it has a `sid` and a `SessionDb` is in the container, belongs to an active session; else the session cookie via `quench_auth::http::routers::ui::get_user_from_req`. No user: `HttpError` 401 `sign in required`. With `SERVICE_AUTH_ENABLED` off `get_user_from_req` yields an `admin` user (see [Configuration](configuration.md)). |

### `guard` check order

1. Method is not `POST`: `405` with `allow: POST` (no body).
2. `content-type` (media type before `;`, case-insensitive) is not `application/json`: `415`.
3. `x-quench-rpc` is not exactly `1`: `403` JSON error `forbidden: missing x-quench-rpc header`.
4. An `Authorization` header is present: allowed (bearer calls skip the origin checks).
5. `Sec-Fetch-Site` present and neither `same-origin` nor `none`: `403` `forbidden: cross-site request`.
6. `Origin` and `Host` both present and their authorities (scheme and trailing `/` removed) differ: `403` `forbidden: cross-origin request`.
7. Otherwise `None`.

Error responses from the guard carry `cache-control: no-store`.

## `testing` (`src/testing.rs`)

A gatehouse-compatible token service for development and tests. Never use it in front of real users.

```rust
#[derive(Clone)]
pub struct StubGatehouse { /* Arc<Inner> */ }       // Debug prints the number of users

impl StubGatehouse {
    #[must_use] pub fn new(users: &[(&str, &str)]) -> Self;
    #[must_use] pub fn jwt(&self) -> JwtConfig;
    pub fn set_access_ttl(&self, secs: i64);
    pub fn revoke_all(&self);
    #[must_use] pub fn login_count(&self) -> usize;
    #[must_use] pub fn refresh_count(&self) -> usize;
}
impl Endpoint for StubGatehouse {}
```

| Item | Behaviour |
|---|---|
| `new(users)` | Accounts as `(username, password)`; uses `JwtConfig::for_tests_with_signing()`. Clones share state. Access TTL starts at 900 s. |
| `jwt()` | The `JwtConfig` that verifies the issued tokens; give it to the service under test (`ContainerBuilder::provide`). |
| `set_access_ttl(secs)` | TTL of newly issued access tokens. Negative values give already-expired tokens; token validation allows 60 s skew, so use `-120` or less. |
| `revoke_all()` | Clears all refresh tokens. |
| `login_count()`, `refresh_count()` | Successful logins and refreshes. |

As an `Endpoint` it answers only `POST` (other methods `405`), and these paths (others `404`):

| Path | Request body | Answer |
|---|---|---|
| `/api/v1/auth/login` | `{"username","password"}` (at most 65 536 bytes) | `200` `{"access_token","refresh_token","token_type":"Bearer","expires_in"}` plus `Set-Cookie` for the session and refresh cookies (`quench_auth::domain::realm`); unknown credentials `401` `{"error":"invalid_credentials"}`; unparsable body `400`. |
| `/api/v1/auth/refresh` | `{"refresh_token"}` | New pair as for login (the old refresh token is removed: it works once); unknown or used token `401` (no body); unparsable `400`. |
| `/api/v1/auth/logout` | `{"refresh_token"}` (optional) | `204`, removes that refresh token, `Set-Cookie` clearing both cookies (`Path=/; Max-Age=0; HttpOnly; SameSite=Lax`). |

Access tokens are real signed tokens with scope `user` and a session id `s0`, `s1`, ... per login. Refresh tokens are 48 hex characters.

## Hidden `__private`

`#[doc(hidden)] pub mod __private { pub use quench_core::{async_trait, submit}; pub use quench_http::prelude::{Endpoint, FromRequest, Request, Response}; }`. Referenced by the `#[server]` expansion.
