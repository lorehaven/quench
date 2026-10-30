---
title: quench-auth
parent: Crates
nav_order: 6
has_children: true
permalink: /crates/quench-auth/
---

`quench-auth` (library `quench_auth`) is the relying-party half of the realm authentication used by the Forge services: everything a service needs to verify a realm token or session issued by gatehouse, check permissions, and send a browser to gatehouse to sign in. It contains no login form, no token-issuing API and no user administration; those live in the gatehouse service. The crate has a framework-agnostic `domain` layer and, behind the `http` feature, a layer built on `quench-http` (middleware, UI helpers, the authorization-code + PKCE client).

| | |
|---|---|
| Version | `0.4.16` |
| Edition | `2024` (workspace) |
| MSRV | not declared |

## Install

```toml
[dependencies]
quench-auth = "0.4"
# without the quench-http layer, Redis and Postgres:
# quench-auth = { version = "0.4", default-features = false }
```

The workspace declares it with `default-features = false`.

## Features

| Feature | Default | Adds |
|---|---|---|
| `http` | yes | Module `http`: `Auth` and `RequireWrite` middleware, `routers::ui`, the SSO request helpers (optional dependency `quench-http`). |
| `redis` | yes | `quench-cache/redis`: `CacheStore` can use Redis, so `SessionDb::from_env` and `JwksVerifier` can share state between replicas. |
| `postgres` | yes | `quench-db/postgres`: the Postgres backend for `UserDb`. |
| `tls` | yes | `https://` for the JWKS and SSO clients (`quench-client/tls`, `quench-http?/tls`), `sslmode=require` (`quench-db/tls`) and `rediss://` (`quench-cache/tls`). |

With `default-features = false` the `domain` layer remains (JWT, sessions, realm conventions, signing, JWKS verifier, token refresh); it still depends on `quench-client` (HTTP calls to gatehouse) but not on `quench-http`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | env access (`env::get_or`), time, UUIDs, hex, base64, random bytes, `async_trait`, `Result`/`err!`/`ensure!`. |
| `quench-crypto` (`argon2`, `jwt`) | Ed25519 JWT encode/decode, Argon2 password hashing, SHA-256. |
| `quench-serde` | `Value`, JSON, `Serialize`/`Deserialize`. |
| `quench-cache` | `CacheStore` for sessions and the JWKS cache. |
| `quench-db` | `Model`, `Repository`, `Db` for `User`/`UserDb`. |
| `quench-proto` | `Cookie` builder for the realm cookies. |
| `quench-client` | HTTP client for JWKS fetch, refresh and the code exchange. |
| `quench-log` | logging. |
| `tokio` | `spawn_blocking` for password verification. |
| `quench-http` (optional, `http`) | `Request`, `Response`, `Middleware`, `Endpoint`. |

No third-party crates beyond the allowed set.

## Layout

| Path | Content | Page |
|---|---|---|
| `quench_auth::prelude` | `Actions`, `Permissions`, `Role`, `User`, `UserDb`, `Claims`, `JwtConfig`, `realm`, `Session`, `SessionDb`, module `jwt`, module `routers` (with `http`) | [api-prelude-and-jwt.md](api-prelude-and-jwt.md) |
| `domain::jwt`, `domain::jwks`, `domain::signing` | tokens, key resolution, key generation | [api-prelude-and-jwt.md](api-prelude-and-jwt.md) |
| `domain::auth`, `domain::session` | users, roles, permissions, sessions | [api-users-and-sessions.md](api-users-and-sessions.md) |
| `domain::realm`, `domain::percent`, `domain::sso_client` | realm conventions, URL encoding, token refresh | [api-realm-and-sso.md](api-realm-and-sso.md) |
| `http::middleware`, `http::routers::ui`, `http::domain` | quench-http layer | [api-http.md](api-http.md) |

## Where to go next

* [Usage](usage.md): wiring a relying party, checking permissions, sessions.
* [Configuration](configuration.md): every environment variable.
* [Internals](internals.md): token format, session keys, refresh and grace-window rules, security properties.
* [Errors](errors.md), [Testing](testing.md).

## Minimal example

```rust
use quench_auth::http::middleware::{auth::Auth, require_write::RequireWrite};
use quench_auth::prelude::JwtConfig;

let config = JwtConfig::init().await;                 // needs GATEHOUSE_URL
use quench_http::middleware::wrap;
let app = wrap(routes, RequireWrite::new(config.clone()));
let app = wrap(app, Auth::new(config));               // outermost: runs first
```

`wrap(inner: Arc<dyn Endpoint>, middleware)` makes the last call the outermost layer. Handlers read the verified `Claims` from the request extensions (`Extension<Claims>`). A complete runnable service is `examples/forge_service_demo`.
