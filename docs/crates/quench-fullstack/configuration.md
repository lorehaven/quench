---
title: Configuration
parent: quench-fullstack
grand_parent: Crates
nav_order: 6
---

The crate has no builder of its own beyond `Ui` and `UiApp` (see [API: pages, assets, Ui, UiApp](api-ui.md)). It reads no environment variables itself. Authentication relies on `quench-auth`'s `JwtConfig`, which the host puts in the `quench-http` container (`ContainerBuilder::provide`), and which reads the following (defaults from `quench-auth/src/domain/jwt.rs`):

| Variable | Default | Effect here |
|---|---|---|
| `SERVICE_NAME` | `service` | The audience name a bearer token must allow (`claims.allows(service_name)`). |
| `SERVICE_AUDIENCES` | the service name | Comma-separated audiences. |
| `SERVICE_REALM` | `https://localhost:8698/token` | Realm. |
| `SERVICE_AUTH_ENABLED` | `false` | When false (development), `quench_auth::http::routers::ui::get_user_from_req` returns an `admin` user (sub `admin`, scope `admin`, audience the service name) for every request without a cookie check; see below. |
| `ACCESS_TOKEN_TTL_SECS` | `900` | Access token lifetime. |
| `REFRESH_TOKEN_TTL_SECS` | `604800` | Refresh token lifetime. |

These are read by `quench-auth`; see its page for the full list. Note the interplay: `User::from_request` tries (1) `Claims` in the request extensions, (2) a valid bearer token, then (3) `get_user_from_req`. An invalid or missing bearer with auth disabled therefore still yields the `admin` user.

## Constants

| Constant | Value |
|---|---|
| `wire::CONTENT_TYPE` | `application/json` |
| `wire::RPC_HEADER` / `RPC_VALUE` | `x-quench-rpc` / `1` |
| `wire::MAX_BODY` | `1 << 20` bytes |
| `assets::PREFIX` | `/_quench/` |
| `document::ROOT_ID` / `STATE_ID` | `app` / `__q_state` |
| `INTERNAL_MESSAGE` | `internal server error` |
| default server function path | `/_fn/<function name>` |
| dev script / stream | `/_quench/dev.js`, `/_quench/dev` |
| asset cache header | `public, max-age=31536000, immutable` |
| gzip threshold | 256 bytes |

## Response headers

* Documents: `cache-control: no-store`, `content-security-policy` (below), `x-content-type-options: nosniff`, `referrer-policy: same-origin`.
* Assets: see [API](api-ui.md).
* Server function answers: `cache-control: no-store`.
* Redirects: `302`, `location`, `cache-control: no-store`.

## Content-Security-Policy

`default-src 'self'; script-src 'self' 'nonce-<N>' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`. `<N>` is a fresh 128-bit value per response, repeated on both script elements.
