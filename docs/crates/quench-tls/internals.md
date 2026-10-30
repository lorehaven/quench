---
title: Internals
parent: quench-tls
grand_parent: Crates
nav_order: 5
---

## Stream semantics

`TlsStream<I>` wraps the transport `I` and a rustls `Connection` (client or server). Instead of an executor-aware library, it adapts the poll interface to rustls' synchronous `read_tls` / `write_tls` through two private wrappers, `SyncRead` and `SyncWrite`. A transport that returns `Pending` is reported to rustls as `io::ErrorKind::WouldBlock`, which the stream converts back into `Poll::Pending` (the waker has already been registered by the transport's `poll_read` / `poll_write`).

### Handshake

`poll_handshake` loops while `session.is_handshaking()`: write queued records, then (if the session wants input) read and process records. A transport EOF during the handshake is `UnexpectedEof`. After the loop the final flight (for example `Finished`) is flushed, so `accept` / `connect` return only when the data is on the wire.

### Reads

`poll_read` first drains plaintext rustls already holds. If it has none it writes any pending records (alerts, key updates), reads and processes more records, and loops. Results:

* plaintext available: copied, `Ready(Ok)`;
* the peer's `close_notify` (reader returns 0): `read_closed` is set; this and every later read returns `Ready(Ok(()))` with no data;
* the transport ends without `close_notify`: `Err(UnexpectedEof)` (after a last attempt to read buffered plaintext), because truncation matters to HTTP framing;
* `process_new_packets` fails: the alert rustls queued is flushed best-effort, then `Err(InvalidData)`.

### Writes

`poll_write` writes plaintext into rustls' send buffer and tries to push records to the transport; it returns the number of bytes accepted (possibly fewer than `buf.len()`). When the buffer is full it first drains it (backpressure). An empty `buf` returns `Ok(0)` immediately. A write error while pushing is reported; `Pending` while pushing is not (the rest goes out on the next write or flush).

`poll_flush` flushes rustls' writer, writes all pending records and flushes the transport (`SyncWrite::flush` itself does nothing; rustls never flushes).

`poll_shutdown` queues `close_notify` once (`close_notify_sent`), writes pending records, then calls the transport's `poll_shutdown`. It does not wait for the peer's `close_notify`.

## Verification

`client_config` uses rustls' normal WebPKI verifier with the root store from `root_store`. With `accept_invalid_certs` it installs `AcceptAnyCert`, a `ServerCertVerifier` that returns `ServerCertVerified::assertion()` for every certificate, name and time, and delegates signature verification to the provider (`verify_tls12_signature`, `verify_tls13_signature`, `supported_verify_schemes`). It therefore protects against nothing.

## Thread-safety

`TlsAcceptor` and `TlsConnector` are `Clone + Send + Sync` (they hold an `Arc` of the rustls config, which is shared across connections). `TlsStream<I>` is `Send` when `I` is `Send`; it needs `&mut` for I/O like any stream. The crate holds no global state; `install_default_provider` touches rustls' process-wide default.

## Provider

Every config is built with `ClientConfig::builder_with_provider(Arc<ring::default_provider()>)` / `ServerConfig::builder_with_provider(..)`, so no default provider has to be installed for this crate's own functions. A new provider `Arc` is created per config build.
