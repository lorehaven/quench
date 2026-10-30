---
title: Errors
parent: quench-auth
grand_parent: Crates
nav_order: 7
---

`quench-auth` defines no error type of its own. It uses three, depending on the layer.

| Where | Type | When |
|---|---|---|
| `JwtConfig::encode_claims`, `decode_claims`, `issue_*`; `signing::encoding_key` | `quench_crypto::jwt::Error` (`kind() -> &ErrorKind`) | `InvalidKeyFormat`: no signer, no active key, or `kid` not resolvable; `InvalidToken`: not a JWT or no `kid`; `InvalidSignature`; `ExpiredSignature`; `ImmatureSignature`: `iat` in the future beyond the 60 s leeway; `MissingRequiredClaim(..)`: `iat` or `exp` missing. Other kinds come from `quench_crypto`. |
| `SessionDb::*`, `JwksVerifier::from_env/new`, `User::new/hash_password` | `quench_core::Result` (`quench_core::Error`) | Cache backend failures (`CacheError` converted with `?`), `GATEHOUSE_URL must be set ..`, HTTP client build errors, `failed to hash password: ..`. |
| `UserDb::get_user`, `validate`; `JwksVerifier::resolve`; `sso_client::refresh` | `Option` | Failures are `None`; the cause is logged or dropped. |

HTTP-layer failures are responses, not errors:

| Situation | Response |
|---|---|
| `Auth`: no or invalid credentials | `401` + `WWW-Authenticate: Bearer`, or `302` to gatehouse login for `Accept: text/html` when `GATEHOUSE_URL` is set |
| `RequireWrite`: no write permission | `403`, empty body |
| `authorize_redirect`, `callback`, `logout_delegation` without `GATEHOUSE_URL` | `503` with text `gatehouse is not configured` |
| `callback` failure | `302` to `<base>/ui/login` |
| `refresh_delegation` failure | `401` |
| `auth_status` | always `200` |
