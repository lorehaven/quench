---
title: Errors
parent: quench-http
grand_parent: Crates
nav_order: 8
---

Three error enums (definitions in [API: request, response, errors](api-request-response.md#error)). Which status each produces:

| Situation | Error | HTTP status | Body |
|---|---|---|---|
| Path parameter cannot be deserialised | `ExtractError::InvalidPath` | 400 | `invalid path parameter: ...` |
| Query string cannot be deserialised | `ExtractError::InvalidQuery` | 400 | `invalid query string: ...` |
| JSON body invalid | `ExtractError::InvalidJson` | 400 | `invalid JSON body: ...` |
| Form body invalid | `ExtractError::InvalidForm` | 400 | `invalid form body: ...` |
| Multipart boundary missing / malformed part / limit hit | `ExtractError::InvalidMultipart` | 400 | `invalid multipart body: ...` |
| Body longer than the limit | `ExtractError::BodyTooLarge(limit)` | **400** (not 413) | `body exceeds the N byte limit` |
| Body read failed | `ExtractError::BodyReadFailed` | 400 | `failed to read request body: ...` |
| `Extension<T>` not set | `HttpError::Status(500, ...)` | 500 | ``` `T` was not set on this request``` |
| `Inject<T>` not in the container | `HttpError::Status(500, DiError text)` | 500 | ``` `T` was requested from the container but is not registered``` |
| WebSocket extractor on a non-upgrade request | `HttpError::Status(400, ...)` | 400 | the protocol error text |
| Unmatched path | (router) | 404 | `not found` |
| Matched path, other method | (router) | 405 | `method not allowed` |
| Path outside a `mount` prefix | (mount) | 404 | `not found` |
| Panicking handler | - | connection closed or 500; server stays up (`security_http` test) | - |

Every error response is `text/plain; charset=utf-8` made by `Response::text`. `HttpError::into_response` maps all `Extraction` errors to 400.

`DiError` never becomes an HTTP response by itself: `ContainerBuilder::build` returns it at startup. Variants: `MissingDependency`, `Duplicate`, `Cycle`, `ConstructionFailed`, `NotFound`. Handle it before serving, for example `build().await.unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"))`.

Compile errors of the macros are listed in [quench-http-macros errors](../quench-http-macros/errors.md).
