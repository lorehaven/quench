---
title: Configuration
parent: quench-proto
nav_order: 5
---

`quench-proto` reads no environment variables and no files (except that the fuzz test reads two, see [Testing](testing.md)). All behaviour is set through plain structs, listed here with their defaults, copied from the `Default` impls.

## `h1::parse::Limits`

| Field | Default | Meaning | Exceeding it |
|---|---|---|---|
| `max_head_bytes` | 65 536 (64 KiB) | start line + headers + the blank line | `ProtoError::HeadersTooLarge` (431) |
| `max_headers` | 100 | header lines | `HeadersTooLarge` (431) |
| `max_uri_bytes` | 8 192 (8 KiB) | the request target | `UriTooLong` (414) |

## `h1::server::ServerConfig`

| Field | Default | Meaning |
|---|---|---|
| `limits` | `Limits::default()` | as above |
| `header_read_timeout` | 30 s | first byte of a request to the end of its head (slowloris guard); `Error::Timeout("reading the request head")` |
| `idle_timeout` | 75 s | a keep-alive connection with nothing buffered may sit this long; ends quietly with `Ok` |
| `write_timeout` | 60 s | per write call (`Error::Timeout("writing")`) |
| `drain_limit` | 1 048 576 (1 MiB) | unread request-body bytes that are discarded so the response is not lost to a reset |
| `drain_timeout` | 2 s | how long that discarding may take |

Constant (not configurable): responses up to 16 KiB are written in the same `write` as their head; the request body travels to the handler through a channel of capacity 1.

## `h1::client::ClientConfig`

| Field | Default | Meaning |
|---|---|---|
| `limits` | `Limits { max_head_bytes: 256 * 1024, ..Limits::default() }` (head 256 KiB, 100 headers, 8 KiB target) | response head limits |
| `response_head_timeout` | 30 s | from sending the request to the full response head (`Error::Timeout("waiting for the response head")`) |
| `write_timeout` | 30 s | per write call while sending the request |

The client has no limit or timeout for the response *body*; use `Body::collect_limited` and your own deadline.

## `h1::decode` constants (fixed)

| Constant | Value | Meaning |
|---|---|---|
| chunk size / trailer line | 4096 bytes | `chunk line too long` |
| all trailers of a message | 16 KiB | `trailers too large` |
| chunk size digits | 16 hex digits | `bad chunk size` |

## `multipart::Limits`

| Field | Default |
|---|---|
| `whole_stream` | `None` (unlimited) |
| `per_field` | `None` (unlimited) |
| `max_header_bytes` | 16 384 |
| `max_fields` | 1024 |

The boundary may be at most 70 bytes (`parse_boundary`).

## `ws::Config`

| Field | Default |
|---|---|
| `max_message_size` | 16 777 216 (16 MiB) reassembled message |
| `max_frame_size` | 16 777 216 (16 MiB) single frame payload (the cap given to `decode_frame`) |

Control frames are limited to 125 bytes by the protocol.

## Fixed limits elsewhere

| Where | Value |
|---|---|
| `Uri::parse` | no length limit of its own (the caller applies `max_uri_bytes`) |
| `HeaderMap` | no limit of its own; `max_headers` is applied by the head parser |
| `cookie` | no limits |
| `sse::Event` | no limits |
