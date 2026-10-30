---
title: Authentication
parent: Guides
nav_order: 7
---

How a Quench service verifies callers with `quench-auth`. Reference: [quench-auth](../crates/quench-auth/index.md). Services are relying parties of the `gatehouse` identity service; tokens are signed JWTs verified against gatehouse's JWKS (no shared secret).

## Claims, roles, scopes

`Claims` carries `sub`, the audience, expiry, an optional session id `sid` and a `scope` string. `Claims::roles` splits the scope on spaces and commas. Permission checks:

* `claims.can(service, action)` is true for a wildcard role (`admin`, `service`) or when the scope contains `service:action`.
* `has_role`, `has_wildcard` test roles; entries containing a colon are ignored for role tests.
* Do not test the scope string with `contains`.

The exact list of methods (including `permissions()`) is in [JWT and claims API](../crates/quench-auth/api-prelude-and-jwt.md).

## Tokens

* Access token: bearer JWT, verified by `JwtConfig` (signature, expiry, audience contains `SERVICE_NAME`).
* Session cookie (`forge_session` by default) and a refresh cookie for browsers; a session-bound token has a `sid` that must be active in `SessionDb`.
* In tests: `JwtConfig::for_tests_with_signing()` and `issue_access_token(sub, scope, None)`; decode with `decode_claims`.

## Setting up a service

```rust
use quench_auth::prelude::{JwtConfig, SessionDb};
use quench_auth::http::middleware::{auth::Auth, require_write::RequireWrite};
use quench_http::prelude::*;

let jwt = JwtConfig::init().await;              // panics unless GATEHOUSE_URL is set
let sessions = SessionDb::from_env().await?;    // Redis if REDIS_URL/CACHE_URL set, else in-process
let container = ContainerBuilder::new()
    .provide(jwt.clone())
    .provide_arc(sessions)
    .build().await?;

let app = wrap(routes, RequireWrite::new(jwt.clone()));
let app = wrap(app, Auth::new(jwt));            // outermost, runs first
```

Environment: `SERVICE_NAME`, `SERVICE_AUDIENCES`, `SERVICE_AUTH_ENABLED` (default `false`: `Auth` passes everything through), `GATEHOUSE_URL`, `GATEHOUSE_CLIENT_ID`, `GATEHOUSE_CLIENT_SECRET`, `AUTH_REDIRECT_HOSTS`. Full list: [configuration](../crates/quench-auth/configuration.md).

## What the middleware does

`Auth`: token from `Authorization: Bearer`, else the session cookie; on success inserts `Claims` into request extensions; on failure tries a silent refresh with the refresh cookie; otherwise redirects browsers (`Accept` contains `text/html`) to the gatehouse login (302) or answers `401` with `WWW-Authenticate: Bearer`. `RequireWrite`: `403` for any method other than `GET`/`HEAD`/`OPTIONS` unless `claims.can(service_name, "write")`.

## In handlers

```rust
use quench_auth::prelude::Claims;

#[delete("/api/notes/{id}")]
async fn remove(Path(id): Path<u64>, Extension(claims): Extension<Claims>) -> Result<String, HttpError> {
    if !claims.can("notes", "delete") {
        return Err(HttpError::status(quench_http::http::StatusCode::FORBIDDEN, "forbidden"));
    }
    Ok(format!("{} deleted {id}", claims.sub))
}
```

`HttpError::status(code, msg)` is in `quench-http/src/error.rs`; `Extension<Claims>` answers 500 when the middleware is not mounted. *Unverified:* the handler listing was not compiled.

## Browser sign-in

Authorization code with PKCE: `login_delegation`, `auth_callback`, `logout_delegation`, `auth_status` in `quench_auth::prelude::routers::ui::pages::auth`; `is_ui_authenticated` and `get_user_from_req` for pages. The service chooses the route paths. `validated_redirect` rejects open redirects. Details and known issues (`auth_status` splits `scope` on commas only; `authorize_redirect` ignores `?redirect=`; `User`'s `Debug` prints the hash): [usage](../crates/quench-auth/usage.md), [realm and SSO API](../crates/quench-auth/api-realm-and-sso.md), `docs/_reports/network-and-data-auth.md`.

## Sessions

`SessionDb::init(CacheStore)`, `create(user, ttl_secs) -> (Session, refresh_token)`, `rotate(refresh_token, ttl)`, `is_active(sid, user)`, `revoke_all(user)`. Storage is `quench-cache`; see [Data access](data-access.md).
