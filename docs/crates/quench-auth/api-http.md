---
title: "API: http layer"
parent: quench-auth
grand_parent: Crates
nav_order: 2
---

Feature `http` (default). Everything here is built on `quench_http` (`Request`, `Response`, `Middleware`, `Endpoint`, the DI container). `SessionDb` is found through the request's container: `req.container().get::<SessionDb>()`; register it once with `ContainerBuilder::provide_arc`.

```rust
// quench_auth::http
pub mod domain;       // cookies, sso_client, plus re-exports of auth, jwks, jwt, realm, session, signing
pub mod middleware;   // auth, require_write
pub mod routers;      // ui
```

## `http::middleware`

Mounting order: `wrap(inner, m)` runs `m` around `inner`, so the last `wrap` is outermost. `RequireWrite` reads the `Claims` that `Auth` stored, so `Auth` is wrapped last:

```rust
let app = wrap(routes, RequireWrite::new(config.clone()));
let app = wrap(app, Auth::new(config));
```

### `middleware::auth::Auth`

```rust
#[derive(Debug)] pub struct Auth { config: JwtConfig }
#[must_use] pub const fn new(config: JwtConfig) -> Self
#[async_trait] impl Middleware for Auth { async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response }
```

Behaviour of `handle`, in order:

1. `config.auth_enabled == false`: call the next endpoint unchanged.
2. Token: `Authorization: Bearer <token>` if present (a header that does not start with `Bearer ` is ignored), otherwise the session cookie (`realm::session_cookie_name()`). The header takes precedence; if it is present but invalid the cookie is **not** tried. The refresh cookie (`realm::refresh_cookie_name()`) is read as a fallback.
3. The token is authenticated (below). On success the `Claims` are inserted with `req.extensions_mut().insert(claims)` and the request continues.
4. If there was no token or it failed: with a refresh cookie, `sso_client::refresh` is called; its fresh access token is authenticated the same way; on success the request continues and the response gets two `Set-Cookie` headers (new session cookie, new refresh cookie). With no refresh cookie (logs a warning if there was no token at all), or if the refresh or the second authentication fails: unauthorized.

Authenticating a token = `decode_claims` ok, `claims.allows(config.service_name)`, and, when the claims carry a `sid`, an active session: `SessionDb::is_active(sid, sub)` is true. If no `SessionDb` is in the container the session is treated as **not active** (warning logged) and the request is unauthorized. Tokens without `sid` skip the session check. Failures are logged at warn level.

Unauthorized response: if the `Accept` header contains `text/html` and `GATEHOUSE_URL` is set, `302` with `Location: <gatehouse>/ui/login?redirect=<percent-encoded request URI>`; otherwise `401` with `WWW-Authenticate: Bearer` and an empty body.

### `middleware::require_write::RequireWrite`

```rust
#[derive(Debug)] pub struct RequireWrite { config: JwtConfig }
#[must_use] pub const fn new(config: JwtConfig) -> Self
#[async_trait] impl Middleware for RequireWrite {}
```
Passes through when `auth_enabled` is false or the method is `GET`, `HEAD` or `OPTIONS`. Any other method needs `Claims` in the extensions with `claims.can(service_name, "write")`; otherwise the response is `403 Forbidden` with no body (a request with no `Claims` at all also gets 403). Logs a warning on refusal.

## `http::routers::ui`

```rust
pub async fn is_ui_authenticated(req: &Request, config: &JwtConfig) -> bool
pub async fn get_user_from_req(req: &Request, config: &JwtConfig) -> Option<Claims>
pub mod pages;   // pages::auth
```

`is_ui_authenticated`: `true` if auth is disabled. Otherwise the session cookie must exist, decode, allow this service, and (if it has a `sid`) belong to an active session. If no `SessionDb` is in the container the token alone is trusted (debug log): this differs from `Auth`, which refuses.

`get_user_from_req`: returns `Claims` already in the request extensions if present (set by `Auth`); with auth disabled, a synthetic admin: `Claims::for_audiences("admin", [service_name], "admin", None, 3600)`; otherwise the same cookie checks as above, `None` on any failure.

## `http::routers::ui::pages::auth`

```rust
#[derive(Debug, Deserialize)] pub struct LoginQuery { pub err: Option<String> }
#[derive(Debug, Serialize)]   pub struct AuthStatus { pub authenticated: bool, pub username: Option<String>, pub roles: Vec<String> }
#[must_use] pub const fn AuthStatus::anonymous() -> Self      // false, None, empty
```
`LoginQuery` models `?err=1` on the login page (not used by any function in this crate).

```rust
pub async fn auth_status(request: &Request, config: &JwtConfig) -> Response
```
Always `200` with JSON `{"authenticated":bool,"username":string|null,"roles":[..]}`. Auth disabled: `true`, username `"dev"`, roles `["admin"]`. No cookie, or a cookie that does not decode: anonymous. Valid cookie: `true`, `sub`, and `roles` = the scope split on spaces and commas (empty items dropped). Reads the cookie and its signature only, not the session store and not the audience.

The page shell's session watcher (`quench-web`, `session_script(resources_prefix, interval_secs)` in `quench-web/src/framework/app/scripts.rs`) calls `<prefix>/status`, and only when it gets a well-formed `authenticated:false` it POSTs `<prefix>/refresh` (the route a service maps to `refresh_delegation`) and navigates to `<prefix>/login` only if that fails. A 404, a network error or an unparsable body are ignored. The service mounts these routes itself.

```rust
pub async fn refresh_delegation(request: &Request) -> Response
```
Reads the refresh cookie and calls `sso_client::refresh`. No cookie or a failed refresh: `401` (empty body). Success: `200` JSON `{"authenticated":true,"username":null,"roles":[]}` with `Set-Cookie` for the new session and refresh cookies.

```rust
pub async fn login_delegation(request: &Request, sso: &SsoConfig) -> Response
```
If the refresh cookie refreshes successfully: `302` to `realm::ui_path("/home")` with the two new cookies. Otherwise `http::domain::sso_client::authorize_redirect`.

```rust
pub async fn auth_callback(request: &Request, sso: &SsoConfig) -> Response
```
`http::domain::sso_client::callback`.

```rust
pub fn logout_delegation(request: &Request) -> Response
```
`302` to `<gatehouse>/ui/logout?redirect=<absolute URL of ui_path("/login") on this host>`; `503` with text `gatehouse is not configured` when `GATEHOUSE_URL` is unset. It does not clear this service's cookies itself (gatehouse performs the logout).

```rust
#[must_use] pub fn redirect_target(request: &Request) -> Option<String>
#[must_use] pub fn validated_redirect(target: &str) -> Option<String>
pub fn allowed_redirect_hosts() -> Vec<String>
```
`redirect_target`: the first `redirect` query parameter, percent-decoded (`None` if absent or not UTF-8) and passed through `validated_redirect`. `validated_redirect`: accepts a path starting with `/` but not `//` and not `/\`; or a URL that starts with one of `allowed_redirect_hosts()` where the rest is empty or starts with `/`, `?` or `#` (so `https://app.example.com` does not admit `https://app.example.com.evil.net/`). Returns `None` otherwise. `allowed_redirect_hosts`: `AUTH_REDIRECT_HOSTS` split on commas, trimmed, trailing `/` removed, empties dropped.

## `http::domain`

```rust
pub mod cookies; pub mod sso_client;
pub use crate::domain::{auth, jwks, jwt, realm, session, signing};
```

### `cookies::cookie_value`

```rust
pub fn cookie_value(req: &Request, name: &str) -> Option<String>
```
The value of the first cookie named `name` in the `Cookie` header (malformed pairs skipped).

### `sso_client`

```rust
pub use crate::domain::sso_client::{RefreshedTokens, SsoConfig, refresh};
#[must_use] pub fn authorize_redirect(request: &Request, config: &SsoConfig) -> Response
pub async fn callback(request: &Request, config: &SsoConfig) -> Response
```

`authorize_redirect`: without `GATEHOUSE_URL`, or when the config is not complete (client id, secret), responds `503` `gatehouse is not configured` (an error is logged). Otherwise generates `state` (24 random bytes, base64url) and a PKCE `code_verifier` (48 random bytes), computes `code_challenge = base64url(SHA-256(code_verifier))` and responds `302` to

```text
<gatehouse>/api/v1/authorize?client_id=..&redirect_uri=..&state=..&code_challenge=..&code_challenge_method=S256
```

(each value percent-encoded; `redirect_uri` is the absolute URL of `ui_path("/auth/callback")`) with `Set-Cookie: forge_authorize_state=<JSON {state, code_verifier, redirect}>` where `redirect` is always `ui_path("/home")`.

`callback` (`GET /ui/auth/callback?code=..&state=..`): needs `GATEHOUSE_URL` (else `503`), the authorize-state cookie (valid JSON), `code` and `state` query parameters, and `state` equal to the saved one. It POSTs a form to `<gatehouse>/api/v1/token`:

| Field | Value |
|---|---|
| `grant_type` | `authorization_code` |
| `code` | the code |
| `redirect_uri` | absolute URL of `ui_path("/auth/callback")` |
| `client_id`, `client_secret` | from `SsoConfig` |
| `code_verifier` | the saved verifier |

and expects `{access_token, refresh_token}`. Success: `302` to the saved `redirect` with three `Set-Cookie`: session, refresh, cleared authorize-state. Any failure (missing or corrupt state cookie, missing code or state, state mismatch, client build error, unreachable gatehouse, non-2xx, unreadable body): `302` to `ui_path("/login")` with the cleared authorize-state cookie (reason logged at warn level).

The absolute URL helper uses `X-Forwarded-Proto` (default `https`) and `X-Forwarded-Host`, falling back to `Host`; the `Forwarded` header is not supported.
