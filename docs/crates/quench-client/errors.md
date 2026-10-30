---
title: Errors
parent: quench-client
grand_parent: Crates
nav_order: 6
---

## `transport::Error`

| Variant | Display text | When |
|---|---|---|
| `Request(String)` | `invalid request: {0}` | unparsable URL (`{url}: {parse error}`), unsupported scheme, relative URL, invalid header name or value (``invalid header `{name}` ``), a `form` body that cannot be serialised |
| `Transport(String)` | `request failed: {0}` | TCP connect failure (`connecting to {host}:{port}: {e}`); TLS handshake failure (`TLS handshake with {host}: {chain}`); invalid TLS server name; HTTP exchange failure (the `quench-proto` error chain joined by `: `); response body larger than `max_response_bytes`; `too many redirects`; unreadable `Location`; ``bad redirect target `{location}`: {e}``; `https` without the `tls` feature; `connection failed` (both attempts failed); a streaming read error |
| `Timeout` | `request timed out` | the request timeout elapsed (`send`: connect, send, redirects and body; `send_streaming`: until the headers) |
| `Decode(String)` | `could not decode response body: {0}` | `Response::json` could not parse the body |
| `Build(String)` | `could not build the client: {0}` | `ClientBuilder::build`: unusable root certificate or client identity, invalid `User-Agent` (``{agent}` is not a valid User-Agent``); blocking: runtime start failure |

`Error` implements `std::error::Error`. There is no `Error::Status`: HTTP error statuses are normal responses at this level.

## Errors from the typed clients

The typed clients return `quench_core::Result`. The error is built with context strings; the `transport::Error` is the underlying cause:

| Message | Cause |
|---|---|
| `Failed to send GET request` / `POST` / `PUT` / `DELETE` | any `transport::Error` while sending |
| `HTTP {status}: {body}` | non-2xx response (`{status}` like `404 Not Found`; `{body}` is the response text or `Unknown error`) |
| `Failed to parse response` | 2xx body is not valid JSON for `T` |
| `client_credentials token request failed` | token request could not be sent |
| `client_credentials grant was refused: {body}` | token endpoint answered non-2xx |
| `failed to parse the client_credentials token response` | token endpoint answered 2xx with an unexpected body |
| `{transport::Error}` text | from `build()` |

`ClientError` (`HttpError`, `RequestFailed`, `ParseError`, `ConnectionError`) is public but never constructed by this crate.

## Retry behaviour

Only one automatic retry exists: a request on a **reused pooled connection** that fails with an I/O error or unexpected EOF is repeated once on a new connection, and only for idempotent methods (`GET`, `HEAD`, `PUT`, `DELETE`, `OPTIONS`, `TRACE`; as decided by `Method::is_idempotent`). `POST` and `PATCH` are never replayed. Nothing else is retried; the typed clients do not retry at all (despite the crate description "with auth and retry support").
