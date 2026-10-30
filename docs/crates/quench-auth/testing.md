---
title: Testing
parent: quench-auth
grand_parent: Crates
nav_order: 9
---

All targets use the `quench-test` harness (`harness = false`). The five targets and what they cover:

| Target | Feature | Covers |
|---|---|---|
| `tests/unit.rs` (modules `domain_auth_tests`, `domain_jwks_tests`, `domain_jwt_tests`, `domain_realm_tests`, `domain_session_tests`, `signing_tests`, `user_db_tests`) | none | Argon2 hashes, roles/permissions/wildcards, lockout counters, `User` model mapping, JWT round trip, audiences and narrowing, expiry and future `iat`, realm names/cookies/base path, session create/rotate/grace window/revoke/index, JWKS fetch + caching against a local server, signing keys |
| `tests/http_middleware_auth.rs` | `http` | `Auth`: disabled bypass, 401 challenge, browser redirect, bad/valid bearer, wrong audience, session-bound tokens |
| `tests/http_middleware_require_write.rs` | `http` | read/write/admin tokens, `GET` without token |
| `tests/http_sso_client.rs` | `http` | `authorize_redirect` and `callback` against a mock gatehouse |
| `tests/http_ui.rs` | `http` | session-cookie authentication, revoked sessions, `get_user_from_req`, `auth_status`, `refresh_delegation`, `login_delegation`, callback and logout redirects, redirect validation |

Tests that touch process-global environment variables (`GATEHOUSE_URL` etc.) serialise on a mutex (`ENV_LOCK`). No test needs Redis or Postgres: sessions use `CacheStore::in_memory()`, and `UserDb` tests run over quench-db's in-memory database (`Db::connect("")`).

## Testing code built on the crate

* `JwtConfig::for_tests()` and `for_tests_with_signing()` are public on purpose; set `auth_enabled`, `service_name`, `audiences` on the returned value.
* `SessionDb::init(CacheStore::in_memory())`, registered with `ContainerBuilder::provide_arc`, makes session-bound tokens verifiable without infrastructure.
* `examples/forge_service_demo` shows the full composition with no gatehouse.
