//! A thin async glue between tokio byte streams and rustls sessions, plus the
//! trust-store and server-config helpers the HTTP server and client share.
//!
//! The crypto backend is `ring`. Server certificates are verified against the
//! Mozilla roots (`webpki-roots`) plus any PEM bundle the operating system
//! provides, so a private CA installed system-wide keeps working.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
pub use rustls::pki_types::ServerName;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, Connection, DigitallySignedStruct, RootCertStore, ServerConfig,
    ServerConnection, SignatureScheme,
};
use std::future::poll_fn;
use std::io::{self, Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// System CA bundles, tried in order when `SSL_CERT_FILE` is unset.
const SYSTEM_BUNDLES: [&str; 5] = [
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/ca-bundle.pem",
    "/etc/ssl/cert.pem",
    "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
];

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// Makes `ring` the process-wide default TLS provider.
///
/// For libraries that build their own rustls configuration and ask for "the default" (a Kubernetes client, an
/// HTTP stack from outside quench). Safe to call from several places and more than once; if something else
/// installed a provider first, that one stays and this does nothing.
pub fn install_default_provider() {
    let _ = ring::default_provider().install_default();
}

/// How a client verifies servers.
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    /// Skip all certificate verification - development only.
    pub accept_invalid_certs: bool,
    /// Extra trust anchors, PEM encoded.
    pub extra_roots_pem: Vec<Vec<u8>>,
    /// Read the operating system's CA bundle too. Off keeps the roots to `webpki-roots` and `extra_roots_pem`.
    pub use_system_roots: bool,
    /// ALPN protocols to offer, most preferred first (e.g. `b"http/1.1"`); none by default.
    pub alpn_protocols: Vec<Vec<u8>>,
}

/// Builds a client config.
///
/// # Errors
///
/// A description of what was wrong with the options or the crypto setup.
pub fn client_config(options: &ClientOptions) -> Result<Arc<ClientConfig>, String> {
    build_client_config(options, None)
}

/// Like [`client_config`], and the client proves who it is with a certificate chain and private key (PEM), for
/// servers that ask for one (mutual TLS: a Kubernetes API server, say).
///
/// # Errors
///
/// A description of what was wrong with the options, the certificate or the key.
pub fn client_config_with_identity(
    options: &ClientOptions,
    certificate_pem: &[u8],
    private_key_pem: &[u8],
) -> Result<Arc<ClientConfig>, String> {
    let chain = pem_certificates(certificate_pem)?;
    if chain.is_empty() {
        return Err("the client certificate PEM holds no certificate".to_string());
    }
    let key = pem_private_key(private_key_pem)?;
    build_client_config(options, Some((chain, key)))
}

fn build_client_config(
    options: &ClientOptions,
    identity: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> Result<Arc<ClientConfig>, String> {
    let provider = provider();
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;
    let mut config = if options.accept_invalid_certs {
        let verified = builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCert(provider)));
        match identity {
            Some((chain, key)) => verified
                .with_client_auth_cert(chain, key)
                .map_err(|e| format!("bad client certificate or key: {e}"))?,
            None => verified.with_no_client_auth(),
        }
    } else {
        let verified = builder.with_root_certificates(root_store(options)?);
        match identity {
            Some((chain, key)) => verified
                .with_client_auth_cert(chain, key)
                .map_err(|e| format!("bad client certificate or key: {e}"))?,
            None => verified.with_no_client_auth(),
        }
    };
    config.alpn_protocols.clone_from(&options.alpn_protocols);
    Ok(Arc::new(config))
}

/// The trust anchors for `options`.
///
/// # Errors
///
/// When an extra PEM root does not parse.
pub fn root_store(options: &ClientOptions) -> Result<RootCertStore, String> {
    let mut roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if options.use_system_roots {
        let path = std::env::var("SSL_CERT_FILE")
            .ok()
            .filter(|p| Path::new(p).is_file())
            .or_else(|| {
                SYSTEM_BUNDLES
                    .iter()
                    .find(|p| Path::new(p).is_file())
                    .map(|p| (*p).to_string())
            });
        if let Some(path) = path
            && let Ok(pem) = std::fs::read(&path)
        {
            // A system bundle can hold certificates rustls rejects; those are skipped, not fatal.
            for cert in CertificateDer::pem_slice_iter(&pem).flatten() {
                let _ = roots.add(cert);
            }
        }
    }
    for pem in &options.extra_roots_pem {
        for cert in CertificateDer::pem_slice_iter(pem) {
            let cert = cert.map_err(|e| format!("bad PEM root certificate: {e}"))?;
            roots
                .add(cert)
                .map_err(|e| format!("unusable root certificate: {e}"))?;
        }
    }
    Ok(roots)
}

/// Loads a server config from a PEM certificate chain and private key; `None`
/// on missing or unreadable files so callers can fall back to plain HTTP in dev.
#[must_use]
pub fn load_server_config(
    cert_path: impl AsRef<Path>,
    key_path: impl AsRef<Path>,
) -> Option<ServerConfig> {
    let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let key = PrivateKeyDer::from_pem_file(key_path).ok()?;
    server_config(chain, key).ok()
}

/// A server config for one certificate chain. Set `alpn_protocols` on the result to negotiate one.
///
/// # Errors
///
/// When the key does not match the chain or is of an unsupported kind.
pub fn server_config(
    chain: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, rustls::Error> {
    ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key)
}

#[derive(Debug)]
struct AcceptAnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Adapts `poll_read` to `io::Read`, reporting `WouldBlock` when it would pend.
struct SyncRead<'a, 'b, I> {
    io: &'a mut I,
    cx: &'a mut Context<'b>,
}

impl<I: AsyncRead + Unpin> Read for SyncRead<'_, '_, I> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_buf = ReadBuf::new(buf);
        match Pin::new(&mut *self.io).poll_read(self.cx, &mut read_buf) {
            Poll::Ready(Ok(())) => Ok(read_buf.filled().len()),
            Poll::Ready(Err(err)) => Err(err),
            Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
        }
    }
}

/// Adapts `poll_write` to `io::Write`, reporting `WouldBlock` when it would pend.
struct SyncWrite<'a, 'b, I> {
    io: &'a mut I,
    cx: &'a mut Context<'b>,
}

impl<I: AsyncWrite + Unpin> Write for SyncWrite<'_, '_, I> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match Pin::new(&mut *self.io).poll_write(self.cx, buf) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
        }
    }

    /// rustls never flushes; the stream flushes the transport itself in `poll_flush`.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A byte stream carried over a TLS session.
pub struct TlsStream<I> {
    io: I,
    session: Connection,
    /// The peer sent `close_notify` (or the transport ended).
    read_closed: bool,
    close_notify_sent: bool,
}

impl<I> std::fmt::Debug for TlsStream<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsStream")
            .field("handshaking", &self.session.is_handshaking())
            .finish_non_exhaustive()
    }
}

impl<I: AsyncRead + AsyncWrite + Unpin> TlsStream<I> {
    const fn new(io: I, session: Connection) -> Self {
        Self {
            io,
            session,
            read_closed: false,
            close_notify_sent: false,
        }
    }

    /// The negotiated ALPN protocol, once the handshake is done.
    #[must_use]
    pub fn alpn_protocol(&self) -> Option<&[u8]> {
        self.session.alpn_protocol()
    }

    /// The underlying transport and the TLS session.
    pub const fn get_ref(&self) -> (&I, &Connection) {
        (&self.io, &self.session)
    }

    /// Writes any TLS records the session has queued.
    fn poll_write_tls(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.session.wants_write() {
            let mut writer = SyncWrite {
                io: &mut self.io,
                cx,
            };
            match self.session.write_tls(&mut writer) {
                Ok(0) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
        Poll::Ready(Ok(()))
    }

    /// Reads TLS records from the transport and processes them. `Ready(Ok(0))` means the transport ended.
    fn poll_read_tls(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let mut reader = SyncRead {
            io: &mut self.io,
            cx,
        };
        let n = match self.session.read_tls(&mut reader) {
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
            Err(err) => return Poll::Ready(Err(err)),
        };
        if let Err(err) = self.session.process_new_packets() {
            // Try to deliver the alert rustls queued before reporting the failure.
            let _ = self.poll_write_tls(cx);
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::InvalidData, err)));
        }
        Poll::Ready(Ok(n))
    }

    fn poll_handshake(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.session.is_handshaking() {
            ready!(self.poll_write_tls(cx))?;
            if !self.session.is_handshaking() {
                break;
            }
            if self.session.wants_read() && ready!(self.poll_read_tls(cx))? == 0 {
                return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
            }
        }
        // Flush the final handshake flight (e.g. the Finished message).
        ready!(self.poll_write_tls(cx))?;
        Poll::Ready(Ok(()))
    }
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncRead for TlsStream<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        loop {
            if this.read_closed {
                return Poll::Ready(Ok(()));
            }
            match this.session.reader().read(buf.initialize_unfilled()) {
                Ok(0) => {
                    this.read_closed = true;
                    return Poll::Ready(Ok(()));
                }
                Ok(n) => {
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(err) => return Poll::Ready(Err(err)),
            }
            // Answer anything the session needs to send (key updates, alerts) before waiting for more.
            let _ = this.poll_write_tls(cx);
            if ready!(this.poll_read_tls(cx))? == 0 {
                // The transport ended without close_notify: surface it, as truncation matters to HTTP.
                match this.session.reader().read(buf.initialize_unfilled()) {
                    Ok(0) => {
                        this.read_closed = true;
                        return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
                    }
                    Ok(n) => {
                        buf.advance(n);
                        return Poll::Ready(Ok(()));
                    }
                    Err(err) => return Poll::Ready(Err(err)),
                }
            }
        }
    }
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncWrite for TlsStream<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        loop {
            let n = this.session.writer().write(buf)?;
            if n > 0 {
                // Push what we can now; the rest goes out on flush or the next write.
                if let Poll::Ready(Err(err)) = this.poll_write_tls(cx) {
                    return Poll::Ready(Err(err));
                }
                return Poll::Ready(Ok(n));
            }
            // rustls' send buffer is full: make room.
            ready!(this.poll_write_tls(cx))?;
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        this.session.writer().flush()?;
        ready!(this.poll_write_tls(cx))?;
        Pin::new(&mut this.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        if !this.close_notify_sent {
            this.session.send_close_notify();
            this.close_notify_sent = true;
        }
        ready!(this.poll_write_tls(cx))?;
        Pin::new(&mut this.io).poll_shutdown(cx)
    }
}

/// Accepts TLS connections with one server config.
#[derive(Clone, Debug)]
pub struct TlsAcceptor(Arc<ServerConfig>);

impl TlsAcceptor {
    #[must_use]
    pub const fn new(config: Arc<ServerConfig>) -> Self {
        Self(config)
    }

    /// Runs the server side of the handshake on `io`.
    ///
    /// # Errors
    ///
    /// An I/O error, or `InvalidData` carrying the rustls failure.
    pub async fn accept<I: AsyncRead + AsyncWrite + Unpin>(
        &self,
        io: I,
    ) -> io::Result<TlsStream<I>> {
        let session = ServerConnection::new(self.0.clone())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut stream = TlsStream::new(io, Connection::Server(session));
        poll_fn(|cx| stream.poll_handshake(cx)).await?;
        Ok(stream)
    }
}

impl From<Arc<ServerConfig>> for TlsAcceptor {
    fn from(config: Arc<ServerConfig>) -> Self {
        Self::new(config)
    }
}

impl From<ServerConfig> for TlsAcceptor {
    fn from(config: ServerConfig) -> Self {
        Self::new(Arc::new(config))
    }
}

/// Opens TLS connections with one client config.
#[derive(Clone, Debug)]
pub struct TlsConnector(Arc<ClientConfig>);

impl TlsConnector {
    #[must_use]
    pub const fn new(config: Arc<ClientConfig>) -> Self {
        Self(config)
    }

    /// Runs the client side of the handshake on `io`, verifying the peer as `server_name`.
    ///
    /// # Errors
    ///
    /// An I/O error, or `InvalidData` carrying the rustls failure (e.g. an untrusted certificate).
    pub async fn connect<I: AsyncRead + AsyncWrite + Unpin>(
        &self,
        server_name: ServerName<'static>,
        io: I,
    ) -> io::Result<TlsStream<I>> {
        let session = ClientConnection::new(self.0.clone(), server_name)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut stream = TlsStream::new(io, Connection::Client(session));
        poll_fn(|cx| stream.poll_handshake(cx)).await?;
        Ok(stream)
    }
}

impl From<Arc<ClientConfig>> for TlsConnector {
    fn from(config: Arc<ClientConfig>) -> Self {
        Self::new(config)
    }
}

/// `ServerName` for a host string (DNS name or IP literal), for [`TlsConnector::connect`].
///
/// # Errors
///
/// When `host` is neither a valid DNS name nor an IP address.
pub fn server_name(host: &str) -> Result<ServerName<'static>, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    ServerName::try_from(host.to_string()).map_err(|e| format!("invalid server name {host:?}: {e}"))
}

/// The certificates in a PEM document.
///
/// # Errors
///
/// A description of the first malformed block.
pub fn pem_certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, String> {
    CertificateDer::pem_slice_iter(pem)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad PEM certificate: {e}"))
}

/// The first private key in a PEM document (PKCS#8, PKCS#1 or SEC1).
///
/// # Errors
///
/// When the document holds no usable key.
pub fn pem_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, String> {
    PrivateKeyDer::from_pem_slice(pem).map_err(|e| format!("bad PEM private key: {e}"))
}

#[cfg(test)]
mod install_tests {
    use super::*;

    #[test]
    fn installing_the_default_provider_is_idempotent() {
        install_default_provider();
        install_default_provider();
        assert!(CryptoProvider::get_default().is_some());
    }
}
