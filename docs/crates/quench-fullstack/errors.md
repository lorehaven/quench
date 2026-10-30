---
title: Errors
parent: quench-fullstack
grand_parent: Crates
nav_order: 7
---

A server function returns `Result<T, ServerError>`.

| Variant | `kind()` | HTTP status | Meaning |
|---|---|---|---|
| `Unauthorized(msg)` | `unauthorized` | 401 | Not signed in, or the session ended. |
| `Forbidden(msg)` | `forbidden` | 403 | Signed in but not allowed; also the protocol checks of `server::guard`. |
| `NotFound(msg)` | `not_found` | 404 | There is no such thing. |
| `Validation { message, fields }` | `validation` | 422 | Input not acceptable; `ServerError::invalid("title", "too short")` names the field. |
| `Internal(msg)` | `internal` | 500 | Server broke; the cause is logged, the browser sees `internal server error`. |
| `Network(msg)` | `network` | none (502 from `status()`) | Browser only: the request could not be made. |
| `BadResponse(msg)` | `bad_response` | none (502) | Browser only: the answer was not what a server function sends (a proxy's HTML page, undecodable result). |

`Display`: `"{kind}: {message}"`.

## Wire body

```json
{"kind":"not_found","message":"no such note","fields":[{"field":"id","message":"unknown"}]}
```

`fields` may be missing when decoding (`#[serde(default)]`). The browser decodes the body back to the same variant (`ServerError::from_response`); a body that names no known kind falls back on the status (422, 401, 403, 404, 500); anything else is `BadResponse`.

## Responses not carrying this body

* `405` with `allow: POST` (wrong method) and `415` (wrong content type) from `server::guard` have no body.
* Extractor failures from `quench-http` (`HttpError`) map through `ServerError::from_http` (see [API: server](api-server.md)).
* Undecodable arguments: `422` `invalid arguments: <serde error>`.
* Oversized body (over `MAX_BODY`): `collect_limited` fails with `ExtractError::BodyTooLarge`, which becomes `HttpError::Extraction` and then `Validation` with the extractor's message, so the response is 422.

## Auth endpoint errors (client)

`Bearer::login/refresh`, `client::sign_in/sign_out`: 401 and 403 give `Unauthorized`, 500+ `Internal`, other statuses `BadResponse`; the message is the answer's `error` string with `_` replaced by a space (for example `invalid credentials`).
