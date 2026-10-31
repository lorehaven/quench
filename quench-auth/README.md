# quench-auth
Authentication and JWT handling for services that rely on realm tokens issued by gatehouse.

## What it is
`quench-auth` is the relying-party half of the realm authentication used by the Forge services: it verifies realm tokens and sessions issued by gatehouse, checks permissions, and sends a browser to gatehouse to sign in. It has no login form, no token-issuing API and no user administration; those live in the gatehouse service.

A framework-agnostic `domain` layer (JWT, sessions, JWKS verifier, token refresh) is always built. Behind the `http` feature a layer on `quench-http` adds middleware, UI helpers and the authorization-code + PKCE client. Sessions and keys are cached with `quench-cache`; gatehouse is called with `quench-client`.

## Use cases
- Protect a service's routes with the `Auth` middleware.
- Require write permission on non-read requests with `RequireWrite`.
- Check a caller's role or scope with `Claims::can`.
- Add browser sign-in through gatehouse (authorization code + PKCE).

## Example
```rust
use quench_auth::prelude::{Claims, JwtConfig, SessionDb};
use quench_auth::http::middleware::{auth::Auth, require_write::RequireWrite};
use quench_http::middleware::wrap;

let jwt = JwtConfig::init().await;            // panics unless GATEHOUSE_URL is set
let sessions = SessionDb::from_env().await?;  // Redis if configured, else in-process

let app = wrap(routes, RequireWrite::new(jwt.clone()));
let app = wrap(app, Auth::new(jwt));          // last wrap = outermost = runs first
```

Reading the caller in a handler:

```rust
#[post("/api/notes")]
async fn create(Extension(claims): Extension<Claims>) -> String {
    if !claims.can("warehouse", "delete") { /* respond 403 */ }
    format!("hello {}", claims.sub)
}
```

## Features
- `http` (default): `Auth` and `RequireWrite` middleware, UI routers, SSO helpers (adds `quench-http`).
- `redis` (default): Redis-backed session and data caches.
- `postgres` (default): the Postgres user store.
- `tls` (default): `https://` for JWKS and SSO clients, `rediss://` and `sslmode=require`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-auth/
