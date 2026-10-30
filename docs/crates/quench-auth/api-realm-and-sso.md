---
title: "API: realm, percent, SSO"
parent: quench-auth
grand_parent: Crates
nav_order: 4
---

## Module `domain::realm`

The only place realm names are constructed. All functions read the environment on each call.

```rust
pub const DEFAULT_AUTH_SCHEMA: &str = "auth";
pub const DEFAULT_SESSION_COOKIE: &str = "forge_session";
pub const DEFAULT_REFRESH_COOKIE: &str = "forge_refresh";
pub const AUTHORIZE_STATE_COOKIE: &str = "forge_authorize_state";
```
`AUTHORIZE_STATE_COOKIE` holds the PKCE verifier, `state` and return destination between the redirect to gatehouse and the callback; it never leaves the service.

```rust
#[must_use] pub fn auth_schema() -> String                  // AUTH_DB_SCHEMA, default "auth"
#[must_use] pub fn session_cookie_name() -> String          // AUTH_COOKIE_NAME, default "forge_session"
#[must_use] pub fn refresh_cookie_name() -> String          // AUTH_REFRESH_COOKIE_NAME, default "forge_refresh"
#[must_use] pub fn cookie_domain() -> Option<String>        // AUTH_COOKIE_DOMAIN; unset/blank is None (host-only)
#[must_use] pub fn gatehouse_url() -> Option<String>        // GATEHOUSE_URL, trimmed, trailing '/' removed; unset/blank is None
#[must_use] pub fn gatehouse_login_url(return_to: Option<&str>) -> Option<String>
#[must_use] pub fn gatehouse_logout_url(return_to: Option<&str>) -> Option<String>
```
The login/logout URLs are `<gatehouse>/ui/login` and `<gatehouse>/ui/logout`, with `?redirect=<percent-encoded return_to>` when `return_to` is non-empty; `None` when `GATEHOUSE_URL` is unset.

```rust
pub fn session_cookie(token: impl Into<String>) -> Cookie
pub fn refresh_cookie(token: impl Into<String>) -> Cookie
#[must_use] pub fn cleared_session_cookie() -> Cookie
#[must_use] pub fn cleared_refresh_cookie() -> Cookie
pub fn authorize_state_cookie(value: impl Into<String>) -> Cookie
#[must_use] pub fn cleared_authorize_state_cookie() -> Cookie
```
(`Cookie` is `quench_proto::cookie::Cookie`.) Attributes, identical everywhere: `Path=/`, `HttpOnly`, `Secure`, `SameSite=Lax` (not `Strict`, which would drop the cookie on the redirect back from gatehouse). Differences:

| Cookie | Domain | Max-Age |
|---|---|---|
| session | `AUTH_COOKIE_DOMAIN` if set | none (browser-session cookie) |
| refresh | `AUTH_COOKIE_DOMAIN` if set | `REFRESH_TOKEN_TTL_SECS` (default 604800; unparsable also 604800) |
| cleared session/refresh | `AUTH_COOKIE_DOMAIN` if set | 0, empty value |
| authorize state | never set | 300 s (5 minutes) |
| cleared authorize state | never set | 0, empty value |

```rust
#[must_use] pub fn base_path() -> String
#[must_use] pub fn ui_path(path: &str) -> String
```
`base_path` reads `BASE_PATH` (default `/`), trims whitespace and trailing slashes, ensures one leading slash; empty or `/` gives `/`. `ui_path("/home")` is `/ui/home` for base `/`, else `<base>/ui/home`.

## Module `domain::percent`

```rust
#[must_use] pub fn encode(input: &str) -> String
#[must_use] pub fn decode(input: &str) -> Option<String>
```
`encode` leaves `A-Z a-z 0-9 - . _ ~` and writes every other byte as uppercase `%XX` (UTF-8 bytes). `decode` turns `%XX` into bytes, keeps a `%` not followed by two hex digits literally, does **not** turn `+` into space, and returns `None` if the result is not UTF-8.

## Module `domain::sso_client`

```rust
#[derive(Clone)]
pub struct SsoConfig {
    pub client_id: String,
    /* pub(crate) client_secret: String, pub(crate) tls_verify: bool */
}
```
`Debug` redacts the secret.

```rust
#[must_use] pub fn init() -> Self
```
Reads `GATEHOUSE_CLIENT_ID` (default empty), `GATEHOUSE_CLIENT_SECRET` (default empty), `GATEHOUSE_TLS_VERIFY` (default `true`; unparsable is `true`). The flow is "configured" only when id, secret and `GATEHOUSE_URL` are all non-empty.

```rust
pub struct RefreshedTokens { pub access_token: String, pub refresh_token: String }
```
`Debug` prints no fields.

```rust
pub async fn refresh(refresh_token: &str) -> Option<RefreshedTokens>
```
POSTs JSON `{"refresh_token": "<token>"}` to `<GATEHOUSE_URL>/api/v1/auth/refresh` (honouring `GATEHOUSE_TLS_VERIFY`, otherwise default client settings) and expects a JSON object with `access_token` and `refresh_token`. `None` on any failure: no `GATEHOUSE_URL`, client build error, network error, non-2xx status, bad JSON.

Crate-private (not API): `AuthorizeState { state, code_verifier, redirect }` (the JSON stored in the authorize-state cookie), `TokenResponse`, `random_urlsafe(bytes)` (base64url-no-pad of random bytes).
