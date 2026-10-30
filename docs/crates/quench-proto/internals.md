---
title: Internals
parent: quench-proto
nav_order: 7
---

Wire rules, framing tables and defences, read from `h1/parse.rs`, `h1/decode.rs`, `h1/server.rs`, `h1/client.rs`, `header.rs`, `ws.rs`, `multipart.rs`, `cookie.rs`.

## Head parsing rules

A head is rejected (never repaired) when any of these hold:

* it does not end in `\r\n\r\n`; a line ends in a bare `\n` (`bare line feed`); a line contains `\r` or NUL (`stray control character`);
* the request line does not have exactly three space-separated parts; the method is not a token; the target does not parse as a `Uri` (so spaces, controls, non-ASCII and `#` are rejected);
* the version is not `HTTP/1.0` or `HTTP/1.1` (other `HTTP/x.y` is 505);
* a header line starts with space or tab (obsolete line folding), has no colon, has whitespace or any non-token byte in the name (so `Name : v` is a `bad header name`), or has a value with a control byte other than tab;
* more than `max_headers` header lines or more than `max_head_bytes` bytes;
* on HTTP/1.1, the number of `Host` headers is not exactly one.

Header values are trimmed of leading and trailing spaces/tabs only. Names are lower-cased. Duplicate headers are kept (`HeaderMap::append`).

## Request body framing (`request_framing`)

| `Transfer-Encoding` | `Content-Length` | HTTP version | Result |
|---|---|---|---|
| absent | absent or `0` | any | no body |
| absent | `n` > 0 | any | fixed length `n` |
| `chunked` (exactly) | absent | 1.1 | chunked |
| `chunked` | present | any | 400 `both Content-Length and Transfer-Encoding` |
| any | any | 1.0 | 400 `Transfer-Encoding on HTTP/1.0` |
| `gzip, chunked` or `chunked, chunked` (last is chunked but not alone) | absent | 1.1 | 501 |
| `chunked, gzip`, `identity`, anything not ending in chunked | absent | 1.1 | 400 |

`Content-Length` parsing: every comma-separated item must be ASCII digits only (no sign, no space inside, not empty) and fit `u64`; repeated headers or list items must all be equal, otherwise 400 `conflicting Content-Length`.

## Response framing

By the **client** (`response_framing`): no body for responses to `HEAD`, for 1xx/204/304, and for a 2xx answer to `CONNECT`; else `Transfer-Encoding: chunked` (exactly) wins even over a `Content-Length`; a `Transfer-Encoding` that is not exactly `chunked` is 400; else `Content-Length` (0 means none); else read until the connection closes.

By the **server** (`serve_connection`, `write_response`):

1. Statuses without body (1xx, 204, 304) and upgrade responses: `Transfer-Encoding` removed; for 1xx and 204 also `Content-Length` removed. No body is written.
2. Else if the handler set a `Content-Length` header that parses: it is used as is (and `Transfer-Encoding` removed); the body must match (`Error::BodyLength` otherwise).
3. Else if the body has an exact length: `Content-Length: n` is set.
4. Else HTTP/1.1 request: `Transfer-Encoding: chunked`.
5. Else (HTTP/1.0, unknown length): the body is delimited by closing; `Connection: close`.
6. `Date` is added unless set (not for upgrades). `Connection: close` is added when the connection will not be kept; `Connection: keep-alive` when an HTTP/1.0 request keeps alive.
7. A response to `HEAD` gets its headers (with `Content-Length` computed) but no body.

## Keep-alive

`wants_keep_alive`: HTTP/1.1 defaults to keep-alive, HTTP/1.0 to close; `Connection: close` wins; `Connection: keep-alive` enables it for 1.0. The server keeps the connection only if the request wanted it, the request body was read to its end cleanly (or fully drained), and shutdown is not signalled. Pipelined requests are served in order; bytes of the next request are never consumed by the previous body decoder.

## Smuggling defences (tested)

* No ambiguity: `Content-Length` + `Transfer-Encoding`, conflicting or malformed lengths, obfuscated `Transfer-Encoding` values, bare LF, obsolete folding, whitespace before the colon, NUL/CR in lines are all 400/501 and the handler is never invoked.
* After a bad request the connection is closed; pipelined requests behind it are never executed.
* Chunk sizes: hex only, at most 16 digits; extensions ignored; no negative or signed sizes.
* Response splitting: `HeaderValue` cannot hold CR or LF, so a handler cannot emit a header that splits the response.
* The decoder gives the same result however the input is split across reads (property-tested).

## Resource limits and denial of service

* Slow headers: `header_read_timeout` (default 30 s). Idle keep-alive: `idle_timeout` (75 s). Non-reading clients: `write_timeout` per write call (60 s). Declared huge bodies are not buffered (the body is streamed with capacity 1; `Body` has no implicit cap, the handler chooses one).
* Header flood: bounded by `max_head_bytes`/`max_headers`. Chunked trailers: 16 KiB. WebSocket: a frame header declaring a huge length is refused (`TooBig`) before any allocation.
* The server has no limit on the number of connections or on request rate: that belongs to the caller (`quench-http`).

## WebSocket frame layout

```text
byte 0:   FIN(1) RSV1(1) RSV2(1) RSV3(1) opcode(4)   RSV bits must be 0 (no extensions)
byte 1:   MASK(1) payload length(7)
          126 -> 2 more bytes (big-endian u16, must be >= 126)
          127 -> 8 more bytes (big-endian u64, top bit 0, must be >= 65536)
[4 bytes masking key, only if MASK]   client -> server frames must be masked, server -> client must not
payload   (XOR with the key[i % 4] when masked)
```

Opcodes: `0` continuation, `1` text, `2` binary, `8` close, `9` ping, `10` pong; anything else is `UnknownOpcode`. Control frames: FIN must be 1, payload at most 125. Close payload: empty, or a big-endian `u16` code (1000-1003, 1007-1011, 3000-4999) plus an optional UTF-8 reason. Handshake: `Sec-WebSocket-Accept = base64(SHA-1(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))`. SHA-1 comes from `quench_crypto::hash::sha1_legacy`; it is a protocol requirement, not a security primitive here.

## Multipart parsing

Delimiter `\r\n--boundary`; the preamble before the first delimiter is skipped; after a delimiter `--` ends the body, otherwise optional transport padding (spaces/tabs) and CRLF start a part; part headers end at a blank line and are limited by `max_header_bytes`; `Content-Disposition` provides `name` and `filename` (quoted-string with backslash escapes, or RFC 5987 `filename*=UTF-8''percent-encoded`). Only a buffer of delimiter length is retained while scanning a field body.

## Cookies

See [api-extras](api-extras.md#cookie). `Set-Cookie` values are built with a fixed attribute order and are not encoded; `Cookie` request headers are split on `;` only (commas are not separators).
