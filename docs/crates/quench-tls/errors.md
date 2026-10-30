---
title: Errors
parent: quench-tls
grand_parent: Crates
nav_order: 4
---

The crate defines no error type. Configuration functions return `Result<_, String>`; stream operations return `std::io::Result`; `server_config` returns `rustls::Error`; `load_server_config` returns `Option`.

## Configuration errors (`String`)

| Function | Message | Cause |
|---|---|---|
| `client_config`, `client_config_with_identity` | rustls' error text | the provider rejected the protocol versions (not expected) |
| `client_config_with_identity` | `the client certificate PEM holds no certificate` | no `CERTIFICATE` block in the certificate PEM |
| `client_config_with_identity`, `pem_certificates` | `bad PEM certificate: {e}` | malformed certificate block |
| `client_config_with_identity`, `pem_private_key` | `bad PEM private key: {e}` | no PKCS#8/PKCS#1/SEC1 key found |
| `client_config_with_identity` | `bad client certificate or key: {e}` | key does not match the chain, or unsupported key |
| `root_store` (via `client_config*`) | `bad PEM root certificate: {e}` | malformed block in `extra_roots_pem` |
| `root_store` | `unusable root certificate: {e}` | certificate rejected as a trust anchor |
| `server_name` | `invalid server name {host:?}: {e}` | neither a DNS name nor an IP |

## `server_config` (`rustls::Error`)

The key does not match the chain, or its type is unsupported.

## Handshake and stream errors (`io::Error`)

| Kind | Where | Cause |
|---|---|---|
| `InvalidData` (source: `rustls::Error`) | `accept`, `connect`, reads and writes | handshake or protocol failure: untrusted certificate, name mismatch, plain text on a TLS port, bad record, alert from the peer. Also returned when `ServerConnection::new` / `ClientConnection::new` reject the config or name. |
| `UnexpectedEof` | handshake | the peer or transport ended before the handshake completed |
| `UnexpectedEof` | `poll_read` | the transport ended **without** `close_notify` (possible truncation attack) |
| `WriteZero` | writes, flush, shutdown | the transport accepted zero bytes while TLS records were pending |
| any other kind | all | passed through from the transport (`WouldBlock` from the transport is turned into `Pending` internally and never surfaces) |

A clean end of stream (peer sent `close_notify`) is `Ok(())` with zero bytes read, repeatedly.
