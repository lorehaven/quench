---
title: Internals
parent: quench-auth
grand_parent: Crates
nav_order: 8
---

## Token format

A JWT with header `{"alg":"EdDSA","kid":"<kid>"}` and payload `{sub, aud:[..], scope, exp, iat, sid?}` (`exp`/`iat` Unix seconds). Verification requires a `kid`, resolves the Ed25519 public key through `KeyResolver`, verifies signature and `exp` (60 s leeway), requires `iat` and `exp`, and rejects `iat` more than the leeway in the future. The audience is checked by callers with `Claims::allows`, and `aud` is a list: gatehouse narrows it to the services the subject may reach, so the audience check also enforces service access. Permission checks (`can`) differentiate `read`/`write` etc.

`scope` carries roles and permissions in one flat list separated by spaces or commas: `admin`, `user sage:read sage:write`. Roles never contain `:`; permissions always do. `has_role`/`has_wildcard` ignore entries with a colon, so a service named `admin` cannot be mistaken for the role.

## Key distribution

Relying parties hold no secret. `JwksVerifier` fetches `/.well-known/jwks.json` from gatehouse (JWK objects with `kid` and base64url Ed25519 `x`) and caches each public key for an hour in the `CacheStore` (prefix `jwks`, key `kid:<kid>`); an unknown `kid` is cached as a miss for 30 seconds so a forged `kid` cannot cause a request per call. On a key rotation, a new `kid` is a cache miss and triggers a refetch of the whole set.

## Session keys

`SessionDb` uses a `CacheStore` with prefix `forge-session` (so Redis keys are `forge-session:session:<sid>` etc.). Four logical keys:

| Key | Value | TTL |
|---|---|---|
| `session:{sid}` | `{ "username", "refresh_hash" }` | refresh lifetime (the `ttl_secs` argument) |
| `refresh:{sha256_hex(token)}` | `sid` (JSON string) | same |
| `user:{username}` | set of sids | same, refreshed by every `create`/`rotate` |
| `reuse:{old_hash}` | `{ "session_id", "username", "refresh_token" }` | 30 s |

Refresh tokens are `<uuid-v4>.<uuid-v4>`; only the SHA-256 hex digest is stored. Rotation consumes the old token with `take` (atomic on Redis), so racing presentations produce one rotation. The `reuse` record lets the same consumed token be answered once more within 30 seconds with the result of the rotation (a client that lost the response); that answer is itself a `take`. This extends the usable life of a captured token by one replay for 30 seconds; the module documentation states the trade-off.

`is_active` is a single `get` of `session:{sid}` per authenticated request, comparing the stored username. Revocation is a delete, so it takes effect for every service that shares the store. Without Redis each process has its own store and revocation is local.

## Passwords and users

Argon2 hashing through `quench_crypto::argon2` (default `Params`). Verification runs in `spawn_blocking` inside `UserDb::validate`. `UserDb` is read-only by design; lockout counters are updated by gatehouse's own login using `User::record_failed_login`.

## Cookies

All realm cookies are `HttpOnly; Secure; SameSite=Lax; Path=/`. Because `Secure` is always set, browsers will not send them over plain `http://` (except to `localhost` in some browsers).

## Security properties

* No `unsafe` code in this crate. Secrets (`SsoConfig` secret, `GeneratedSigningKey` private key, `RefreshedTokens`) have hand-written `Debug` that redacts them; `User` is the exception: its derived `Debug` includes `password` (the hash) and `mfa_secret`.
* PKCE: S256 only; `state` compared with `==` (not constant-time) against the cookie.
* Open redirects: `validated_redirect` (above). Note `authorize_redirect` ignores `?redirect=` and always returns to `ui_path("/home")`.
* `GATEHOUSE_TLS_VERIFY=false` disables certificate checks for the JWKS fetch, refresh and code exchange.
