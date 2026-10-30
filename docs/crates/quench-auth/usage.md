---
title: Usage
parent: quench-auth
grand_parent: Crates
nav_order: 1
---

Examples follow `examples/forge_service_demo` and the crate's tests.

## 1. Set up a relying party

```rust
use quench_auth::prelude::{JwtConfig, SessionDb};
use quench_http::di::ContainerBuilder;

let jwt = JwtConfig::init().await;            // panics unless GATEHOUSE_URL is set
let sessions = SessionDb::from_env().await?;  // Redis if REDIS_URL/CACHE_URL is set, else in-process
let container = ContainerBuilder::new()
    .provide(jwt.clone())
    .provide_arc(sessions)                    // SessionDb is looked up in the DI container
    .build().await?;
```

Environment (full list in [Configuration](configuration.md)): `SERVICE_NAME`, `SERVICE_AUDIENCES`, `SERVICE_AUTH_ENABLED=true`, `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`.

`JwtConfig::init()` builds a `JwksVerifier` that fetches gatehouse's keys from `<GATEHOUSE_URL>/.well-known/jwks.json`; nothing is shared secret. Pitfall: `SERVICE_AUTH_ENABLED` defaults to `false`, which turns `Auth` into a pass-through.

## 2. Mount the middleware

```rust
use quench_auth::http::middleware::{auth::Auth, require_write::RequireWrite};
use quench_http::middleware::wrap;

let app = wrap(routes, RequireWrite::new(jwt.clone()));
let app = wrap(app, Auth::new(jwt));     // last wrap = outermost = runs first
```

`Auth` takes the token from `Authorization: Bearer <token>`, else from the session cookie (`forge_session` by default). It verifies signature and expiry, checks the audience contains `SERVICE_NAME`, and if the token has a `sid` checks the session is active in the `SessionDb` from the DI container. On success the `Claims` are inserted into the request extensions. If verification fails and a refresh cookie is present, it silently exchanges it at gatehouse and sets fresh cookies on the response. Otherwise: a browser (an `Accept` header containing `text/html`) is redirected (302) to the gatehouse login with `?redirect=<original uri>` when `GATEHOUSE_URL` is set; everything else gets `401` with `WWW-Authenticate: Bearer`.

`RequireWrite` answers `403` to any non-`GET`/`HEAD`/`OPTIONS` request whose `Claims` do not `can(service_name, "write")`.

## 3. Read the caller and check a permission

```rust
use quench_auth::prelude::Claims;
use quench_http::prelude::Extension;

#[post("/api/notes")]
async fn create(Extension(claims): Extension<Claims>) -> String {
    if !claims.can("warehouse", "delete") { /* respond 403 */ }
    format!("hello {}", claims.sub)
}
```

`can(service, action)` is true for a wildcard role (`admin`, `service`) or when the scope carries `service:action`. Never test the scope with `contains`: use `has_role`, `can`, `has_wildcard` (they split the claim into entries and ignore entries containing a colon for role tests).

## 4. Browser sign-in (authorization code + PKCE)

```rust
use quench_auth::prelude::routers::ui::pages::auth::{login_delegation, auth_callback, logout_delegation, auth_status};
use quench_auth::http::domain::sso_client::SsoConfig;

let sso = SsoConfig::init();           // GATEHOUSE_CLIENT_ID / _SECRET / _TLS_VERIFY
// GET  /ui/login          -> login_delegation(&request, &sso).await
// GET  /ui/auth/callback  -> auth_callback(&request, &sso).await
// GET  /ui/logout         -> logout_delegation(&request)
// GET  /ui/status         -> auth_status(&request, &jwt).await
```

`login_delegation` first tries the refresh cookie (renewal without a login page, then 302 to `<base>/ui/home`); otherwise `authorize_redirect` sends the browser (302) to `<GATEHOUSE_URL>/api/v1/authorize` with a PKCE challenge and stores the verifier and `state` in a 5-minute cookie. `auth_callback` checks `state`, exchanges the code at `<GATEHOUSE_URL>/api/v1/token`, sets the two session cookies and redirects to `<base>/ui/home`. On any failure the callback redirects to `<base>/ui/login` and clears the state cookie. Paths of the routes are chosen by the service; these functions only produce responses. *Unverified:* the route table above is the convention used by `realm::ui_path`, not enforced by the crate.

## 5. Protect a page

```rust
use quench_auth::http::routers::ui::{is_ui_authenticated, get_user_from_req};

if !is_ui_authenticated(&request, &jwt).await { /* redirect to login */ }
let claims = get_user_from_req(&request, &jwt).await;   // Option<Claims>
```

`is_ui_authenticated` reads the cookie, verifies it, checks the audience and, for tokens with a `sid`, the session store (if a `SessionDb` is registered; if none is registered the token alone is trusted). With auth disabled it returns `true`.

## 6. Sessions (gatehouse side, or tests)

```rust
let sessions = SessionDb::init(CacheStore::in_memory());
let (session, refresh_token) = sessions.create("alice", 604_800).await?;
let rotated = sessions.rotate(&refresh_token, 604_800).await?;   // Option<(Session, String)>
assert!(sessions.is_active(&session.id, "alice").await?);
sessions.revoke_all("alice").await?;                              // number of live sessions ended
```

## 7. Issue tokens in tests

```rust
let jwt = JwtConfig::for_tests_with_signing();   // fresh in-memory Ed25519 key
let token = jwt.issue_access_token("alice".into(), "user sage:read".into(), None).await?;
let claims = jwt.decode_claims(&token).await?;
assert!(claims.can("sage", "read"));
```

`for_tests()` resolves and signs nothing. Both read `SERVICE_NAME`, `SERVICE_AUDIENCES`, `SERVICE_AUTH_ENABLED` from the environment, so set them first (or assign the public fields afterwards, as the demo does).

## 8. Open-redirect-safe `?redirect=`

```rust
use quench_auth::http::routers::ui::pages::auth::{redirect_target, validated_redirect};
assert_eq!(validated_redirect("/ui/home"), Some("/ui/home".into()));
assert_eq!(validated_redirect("//evil.example"), None);
```

An absolute URL passes only when it starts with a prefix from `AUTH_REDIRECT_HOSTS` followed by end, `/`, `?` or `#`.
