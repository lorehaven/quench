---
title: Errors
parent: quench-proto
nav_order: 6
---

Each module has its own error type; none is converted into another except as shown.

## `quench_proto::Error` (connection drivers)

Returned by `h1::server::serve_connection`, `h1::client::send_request`, `h1::reader::Reader::poll_body`.

| Variant | When |
|---|---|
| `Io(io::Error)` | socket error |
| `Proto(ProtoError)` | the peer broke the HTTP/1.1 rules (server: after answering with the matching status) |
| `Timeout(&'static str)` | `"reading the request head"`, `"writing"`, `"waiting for the response head"` |
| `UnexpectedEof` | the peer closed in the middle of a head or body |
| `BodyLength` | a body produced a different number of bytes than its declared `Content-Length` |
| `Body(BoxError)` | a response/request body stream failed while being written |

## `h1::parse::ProtoError`

| Variant | HTTP status | Typical `BadRequest` reasons |
|---|---|---|
| `BadRequest(&str)` | 400 | `head not terminated`, `bare line feed`, `stray control character`, `empty request`, `malformed request line`, `bad method`, `bad request target`, `bad HTTP version`, `obsolete line folding`, `header without a colon`, `bad header name`, `bad header value`, `missing Host header`, `duplicate Host header`, `empty response`, `malformed status line`, `bad status code`, `bad Content-Length`, `conflicting Content-Length`, `Transfer-Encoding on HTTP/1.0`, `both Content-Length and Transfer-Encoding`, `Transfer-Encoding does not end in chunked`, `bad Transfer-Encoding` (response), chunk errors from `h1::decode` |
| `UriTooLong` | 414 | target over `max_uri_bytes` |
| `HeadersTooLarge` | 431 | head over `max_head_bytes` or more than `max_headers` lines |
| `VersionNotSupported` | 505 | `HTTP/0.9`, `HTTP/2.0`, `HTTP/1.2`, any `HTTP/x` other than 1.0 and 1.1 |
| `NotImplemented` | 501 | `Transfer-Encoding: gzip, chunked` (a coding other than bare `chunked`) |

## Message-type errors

| Type | Display | Raised by |
|---|---|---|
| `method::InvalidMethod` | `invalid HTTP method` | `Method::from_bytes`, `FromStr`, `TryFrom` |
| `status::InvalidStatusCode` | `invalid status code` | `StatusCode::from_u16`, `from_bytes`, `TryFrom<u16>` |
| `header::InvalidHeaderName` | `invalid HTTP header name` | `HeaderName::from_bytes`, `FromStr`, `TryFrom<&str>` |
| `header::InvalidHeaderValue` | `failed to parse header value` | `HeaderValue::from_bytes`, `from_str` |
| `header::ToStrError` | `header value is not visible ASCII` | `HeaderValue::to_str` |
| `uri::InvalidUri` | `Empty`, `InvalidChar`, `InvalidScheme`, `InvalidAuthority`, `InvalidPort` (see source for texts) | `Uri::parse`, `resolve` |
| `body::CollectError` | `TooLarge(limit)`, `Read(err)` | `Body::collect_limited` |
| `cookie::ParseError` | `MissingPair`, `EmptyName` | `Cookie::parse` |
| `multipart::Error` | see [api-extras](api-extras.md#multipart-feature-multipart) | the multipart reader |

`HeaderName::from_static` / `HeaderValue::from_static` / `Method`-style constants **panic** on invalid input (they are for literals).

## WebSocket

`ws::HandshakeError` texts are listed with `validate_upgrade`. `ws::ProtocolError` Display:

| Variant | Text | Close code sent |
|---|---|---|
| `ReservedBits` | reserved bits set without a negotiated extension | 1002 |
| `UnknownOpcode` | unknown opcode | 1002 |
| `FragmentedControlFrame` | fragmented control frame | 1002 |
| `ControlFrameTooLong` | control frame payload over 125 bytes | 1002 |
| `BadMasking` | wrong masking for this direction | 1002 |
| `UnexpectedContinuation` | continuation frame without a message to continue | 1002 |
| `InterruptedMessage` | data frame inside a fragmented message | 1002 |
| `NonMinimalLength` | payload length not minimally encoded | 1002 |
| `InvalidClosePayload` | malformed close payload | 1007 |
| `InvalidCloseCode` | invalid close status code | 1002 |
| `InvalidUtf8` | text is not valid UTF-8 | 1007 |
| `TooBig` | message too big | 1009 |

`ws::Error` Display: `websocket i/o error: {e}`, `websocket protocol error: {e}`, `websocket is closed`, `control payload over 125 bytes`.
