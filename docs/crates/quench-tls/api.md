---
title: API
parent: quench-tls
grand_parent: Crates
nav_order: 2
---

Reference for the whole public API (source: `src/lib.rs`; everything is at the crate root). Errors are described in [Errors](errors.md).

## Re-export

```rust
pub use rustls::pki_types::ServerName;
```

The type of a name to verify the peer as; `TlsConnector::connect` takes `ServerName<'static>`. Build one with [`server_name`](#server_name).

## `install_default_provider`

```rust
pub fn install_default_provider()
```

Makes `ring` the process-wide default rustls crypto provider. For libraries that build their own rustls configuration and ask for "the default" (a Kubernetes client, an HTTP stack from outside Quench), which rustls cannot choose when more than one backend is compiled in. Safe to call repeatedly and from several places: if another provider was installed first it stays and the call does nothing (the result of `install_default` is ignored). Not needed for the functions of this crate, which pass the provider explicitly.

## `ClientOptions`

```rust
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub accept_invalid_certs: bool,
    pub extra_roots_pem: Vec<Vec<u8>>,
    pub use_system_roots: bool,
    pub alpn_protocols: Vec<Vec<u8>>,
}
```

| Field | Default (`Default`) | Meaning |
|---|---|---|
| `accept_invalid_certs` | `false` | skip all certificate verification (development only) |
| `extra_roots_pem` | empty | extra trust anchors, one PEM document per element (each may hold several certificates) |
| `use_system_roots` | **`false`** | also read the operating system's CA bundle. With `false` the roots are only `webpki-roots` plus `extra_roots_pem`. |
| `alpn_protocols` | empty | ALPN protocols to offer, most preferred first, e.g. `b"http/1.1".to_vec()` |

## `client_config`

```rust
pub fn client_config(options: &ClientOptions) -> Result<Arc<ClientConfig>, String>
```

Builds a rustls `ClientConfig` with the `ring` provider and the safe default protocol versions (TLS 1.2 and 1.3), no client authentication, and `alpn_protocols` from `options`. With `accept_invalid_certs` the verifier accepts any server certificate and any name (signatures in the handshake are still checked structurally with the provider's algorithms) and **no root store is loaded**. Otherwise the trust anchors come from [`root_store`](#root_store). `Err` carries a description of the crypto set-up or root-certificate problem.

## `client_config_with_identity`

```rust
pub fn client_config_with_identity(
    options: &ClientOptions,
    certificate_pem: &[u8],
    private_key_pem: &[u8],
) -> Result<Arc<ClientConfig>, String>
```

Like `client_config`, and the client presents the certificate chain and private key (PEM) to servers that request a client certificate (mutual TLS). Errors: `"the client certificate PEM holds no certificate"`, `"bad PEM certificate: {e}"`, `"bad PEM private key: {e}"`, `"bad client certificate or key: {e}"` (key does not match, unsupported key).

## `root_store`

```rust
pub fn root_store(options: &ClientOptions) -> Result<RootCertStore, String>
```

The trust anchors for `options`, in this order:

1. all of `webpki_roots::TLS_SERVER_ROOTS`;
2. if `use_system_roots`: the bundle named by `SSL_CERT_FILE` when that variable is set to an existing file; otherwise the first existing file among `/etc/ssl/certs/ca-certificates.crt`, `/etc/pki/tls/certs/ca-bundle.crt`, `/etc/ssl/ca-bundle.pem`, `/etc/ssl/cert.pem`, `/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem`. Only one file is read. Unreadable files and certificates rustls rejects are skipped silently;
3. each PEM in `extra_roots_pem`: any malformed block or unusable certificate is an error (`"bad PEM root certificate: {e}"` / `"unusable root certificate: {e}"`).

`accept_invalid_certs` is ignored by this function.

## `load_server_config`

```rust
#[must_use]
pub fn load_server_config(
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> Option<ServerConfig>
```

Reads a PEM certificate chain and a PEM private key (PKCS#8, PKCS#1 or SEC1) from files and builds a server config with [`server_config`](#server_config). Returns `None` for any failure (missing or unreadable file, bad PEM, no usable key, key/chain mismatch) so callers can fall back to plain HTTP in development. The reason is lost; use `pem_certificates`, `pem_private_key` and `server_config` for diagnostics.

## `server_config`

```rust
pub fn server_config(
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, rustls::Error>
```

A server config for one certificate chain: `ring` provider, safe default protocol versions, **no client authentication**, single certificate. `alpn_protocols` is empty; set `config.alpn_protocols` on the result to negotiate one. `Err` when the key does not match the chain or is unsupported. There is no helper for servers that require client certificates; build the rustls `ServerConfig` yourself (the tests do) and wrap it in `TlsAcceptor`.

## `server_name`

```rust
pub fn server_name(host: &str) -> Result<ServerName<'static>, String>
```

A `ServerName` from a DNS name or an IP literal. One leading `[` and trailing `]` are stripped (so `"[::1]"` works). `Err("invalid server name {host:?}: {e}")` otherwise.

## `pem_certificates`

```rust
pub fn pem_certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, String>
```

All certificates in a PEM document, in order; other PEM blocks are skipped. An empty vector for a document without certificates. `Err("bad PEM certificate: {e}")` for the first malformed block.

## `pem_private_key`

```rust
pub fn pem_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, String>
```

The first private key (PKCS#8, PKCS#1 or SEC1) in a PEM document. `Err("bad PEM private key: {e}")` when there is none.

## `TlsAcceptor`

```rust
#[derive(Clone, Debug)]
pub struct TlsAcceptor(Arc<ServerConfig>);
```

Accepts TLS connections with one server config. Cheap to clone (shares the `Arc`).

| Item | Signature |
|---|---|
| `new` | `#[must_use] pub const fn new(config: Arc<ServerConfig>) -> Self` |
| `accept` | `pub async fn accept<I: AsyncRead + AsyncWrite + Unpin>(&self, io: I) -> io::Result<TlsStream<I>>` |
| `From<Arc<ServerConfig>>` | `fn from(config: Arc<ServerConfig>) -> Self` |
| `From<ServerConfig>` | `fn from(config: ServerConfig) -> Self` |

`accept` runs the server handshake to completion on `io` and returns the established stream. Errors: an I/O error from the transport, `UnexpectedEof` when the peer closes mid-handshake, or `InvalidData` carrying the rustls error (plain text sent to a TLS port, protocol or certificate failures; a failure alert is flushed best-effort first).

## `TlsConnector`

```rust
#[derive(Clone, Debug)]
pub struct TlsConnector(Arc<ClientConfig>);
```

| Item | Signature |
|---|---|
| `new` | `#[must_use] pub const fn new(config: Arc<ClientConfig>) -> Self` |
| `connect` | `pub async fn connect<I: AsyncRead + AsyncWrite + Unpin>(&self, server_name: ServerName<'static>, io: I) -> io::Result<TlsStream<I>>` |
| `From<Arc<ClientConfig>>` | `fn from(config: Arc<ClientConfig>) -> Self` |

`connect` runs the client handshake, verifying the peer as `server_name`. Errors as for `accept`; an untrusted or mismatched certificate is `InvalidData` carrying the rustls error.

## `TlsStream`

```rust
pub struct TlsStream<I> { /* io: I, session: Connection, read_closed, close_notify_sent */ }
impl<I> Debug for TlsStream<I>          // prints only `handshaking`
impl<I: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsStream<I>
impl<I: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsStream<I>
```

An established (or, internally, handshaking) TLS session over a transport `I`. Obtained only from `TlsAcceptor::accept` / `TlsConnector::connect`; there is no public constructor.

| Method | Signature | Notes |
|---|---|---|
| `alpn_protocol` | `#[must_use] pub fn alpn_protocol(&self) -> Option<&[u8]>` | the negotiated ALPN protocol, `None` if none |
| `get_ref` | `pub const fn get_ref(&self) -> (&I, &Connection)` | the transport and the rustls `Connection` (peer certificates, protocol version, ...) |

Behaviour of the trait implementations: see [Internals](internals.md#stream-semantics).

## Public dependencies in signatures

`Arc<ClientConfig>`, `ServerConfig`, `RootCertStore`, `Connection`, `rustls::Error`, `CertificateDer`, `PrivateKeyDer` are rustls types. The crate does not re-export `rustls` (only `ServerName`), so callers that name those types need `rustls` 0.23 as their own dependency, with a compatible version.
