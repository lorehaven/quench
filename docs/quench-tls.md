# Quench TLS

`quench-tls` (crate `quench_tls`) is the small layer between tokio byte streams and [`rustls`](https://github.com/rustls/rustls) that [Quench Proto](./quench-proto.md) (HTTP server and client) and [Quench Mail](./quench-mail.md) (SMTP over TLS) share. It replaces `tokio-rustls`, `hyper-rustls`, `rustls-platform-verifier` and the copies of root-store setup that used to live in each caller. `ring` is the only crypto backend; `aws-lc-rs` is not built.

## API

- **`TlsAcceptor`** / **`TlsConnector`** — `accept(io)` and `connect(server_name, io)` run the handshake and return a **`TlsStream<I>`** that implements `AsyncRead + AsyncWrite`. It buffers like any rustls stream, flushes queued records (alerts, key updates) before waiting for more input, sends `close_notify` on shutdown, and reports a peer that vanished without `close_notify` as `UnexpectedEof`, since truncation matters to HTTP framing. `alpn_protocol()` and `get_ref()` expose the negotiated state.
- **`client_config(&ClientOptions)`** — a client config: trust anchors from `webpki-roots`, the operating system's bundle (`use_system_roots`: `SSL_CERT_FILE`, then the usual `/etc/ssl` paths) and any `extra_roots_pem`, ALPN protocols from `alpn_protocols`. `accept_invalid_certs` skips verification entirely and is for development only.
- **`load_server_config(cert, key)`** — a PEM certificate chain and private key to a `ServerConfig`; `None` if either file is missing or unusable, so callers can fall back to plain HTTP in dev. `server_config(chain, key)` takes parsed values; `pem_certificates` / `pem_private_key` parse PEM.
- **`client_config_with_identity(&ClientOptions, cert_pem, key_pem)`** — like `client_config`, and the client presents a certificate chain and private key to servers that ask (mutual TLS); a missing or mismatched key is described in the error.
- **`install_default_provider()`** — makes `ring` the process-wide rustls provider, for a library that builds its own rustls configuration and asks for "the default" (rustls cannot choose one when more than one backend is compiled in). Idempotent; an already-installed provider stays.
- **`server_name(host)`** — a DNS name or IP literal (brackets allowed) as a `ServerName`.

TLS 1.2 and 1.3 are enabled with rustls' safe defaults.

## Testing

`tests/fixtures` holds a throwaway CA and a `localhost` / `127.0.0.1` leaf (ECDSA P-256, valid for a century; the CA key was discarded). The tests cover a full handshake both ways, large transfers through a tiny pipe, untrusted and mismatched certificates, TLS 1.2, truncation, and a transport that fails mid-handshake or mid-stream.

[Home](../README.md)
