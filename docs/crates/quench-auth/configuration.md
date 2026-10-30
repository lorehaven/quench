---
title: Configuration
parent: quench-auth
grand_parent: Crates
nav_order: 6
---

All settings are environment variables, read through `quench_core::env` (unset, or not valid UTF-8, counts as unset). There are no config files. Secrets: only `GATEHOUSE_CLIENT_SECRET` (never logged: `SsoConfig`'s `Debug` redacts it).

| Variable | Type | Default | Read by | Meaning |
|---|---|---|---|---|
| `SERVICE_NAME` | string | `service` | `JwtConfig` constructors | This service's audience name. |
| `SERVICE_REALM` | string | `https://localhost:8698/token` | `JwtConfig` constructors | `JwtConfig.realm`. |
| `SERVICE_AUDIENCES` | comma list | `[SERVICE_NAME]` | `JwtConfig` constructors | Audiences a token issued here covers; gatehouse lists the realm, a relying party only itself. |
| `SERVICE_AUTH_ENABLED` | `true`/`false` | `false` | `JwtConfig` constructors | `false` disables `Auth`, `RequireWrite`, `is_ui_authenticated`. Any value other than the exact text `true` or `false` counts as `false`. |
| `ACCESS_TOKEN_TTL_SECS` | integer | `900` | `JwtConfig` constructors | Lifetime of issued access tokens. |
| `REFRESH_TOKEN_TTL_SECS` | integer | `604800` | `JwtConfig` constructors, `realm::refresh_cookie` | Refresh lifetime; Max-Age of the refresh cookie. |
| `GATEHOUSE_URL` | URL | unset | `realm::gatehouse_url`, `JwksVerifier::from_env`, `sso_client` | Gatehouse base URL (trailing `/` removed). Mandatory for `JwtConfig::init()` (panics without) and `JwksVerifier::from_env`. |
| `GATEHOUSE_TLS_VERIFY` | bool | `true` | `JwksVerifier::from_env`, `SsoConfig::init`, `sso_client::refresh` | `false` accepts invalid certificates. |
| `GATEHOUSE_CLIENT_ID` | string | empty | `SsoConfig::init` | OAuth client id of this service. |
| `GATEHOUSE_CLIENT_SECRET` | string | empty | `SsoConfig::init` | OAuth client secret. |
| `AUTH_DB_SCHEMA` | string | `auth` | `realm::auth_schema` | Schema of the `users` table. |
| `AUTH_COOKIE_NAME` | string | `forge_session` | `realm::session_cookie_name` | |
| `AUTH_REFRESH_COOKIE_NAME` | string | `forge_refresh` | `realm::refresh_cookie_name` | |
| `AUTH_COOKIE_DOMAIN` | string | unset (host-only) | `realm::cookie_domain` | Parent domain of the session and refresh cookies, e.g. `.forge.example.com`. |
| `AUTH_REDIRECT_HOSTS` | comma list | empty | `allowed_redirect_hosts` | URL prefixes a `?redirect=` may point at. |
| `BASE_PATH` | path | `/` | `realm::base_path`, `ui_path` | Mount point of this service. |
| `CACHE_URL`, `REDIS_URL` | URL list | unset | `SessionDb::from_env`, `JwksVerifier::new` (through `CacheStore::from_env`) | Shared Redis for sessions (key prefix `forge-session`) and the JWKS cache (`jwks`). Unset: in-process. |
| `CACHE_CLUSTER`, `REDIS_CLUSTER` | `true` | unset | same | Force cluster mode. See [quench-cache configuration](../quench-cache/configuration.md). |

Constants that are not configurable: JWKS positive cache 3600 s, negative cache 30 s, JWKS HTTP timeout 5 s, refresh reuse grace window 30 s, authorize-state cookie 5 minutes.

Postgres connection settings belong to `quench-db`, not to this crate.
