use crate::body::InboundBody;
use crate::di::Container;
use crate::endpoint::Endpoint;
use crate::request::Request;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto;
use hyper_util::server::graceful::GracefulShutdown;
use rustls::ServerConfig as TlsConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// How long [`serve_http`]/[`serve_https`] wait, after a shutdown signal,
/// for in-flight connections to drain on their own before forcing an exit.
/// Kept comfortably under Kubernetes' default 30s
/// `terminationGracePeriodSeconds`, so the process exits on its own instead
/// of being SIGKILLed mid-drain.
pub const DEFAULT_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(25);

/// How long a connection may take sending its request headers before it's
/// dropped. Without this, a client that opens a connection and then sends
/// headers one byte at a time (or never finishes them) ties up a
/// connection slot forever - the "slowloris" class of resource-exhaustion
/// attack. Generous enough for a slow mobile client, tight enough that a
/// deliberately stalled connection can't accumulate.
pub const DEFAULT_HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

fn connection_builder() -> auto::Builder<TokioExecutor> {
    let mut builder = auto::Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(DEFAULT_HEADER_READ_TIMEOUT);
    builder
}

/// Serves `app` over plain HTTP on `addr`. Each connection is handled on
/// its own task and can pipeline HTTP/1.1 or negotiate HTTP/2
/// automatically. Runs until a `SIGTERM`/`SIGINT` (or, on non-Unix,
/// Ctrl+C) is received, at which point it stops accepting new connections
/// and waits up to [`DEFAULT_GRACEFUL_TIMEOUT`] for in-flight ones to
/// finish before returning.
pub async fn serve_http(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    addr: SocketAddr,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_http_on(app, container, listener, shutdown_signal()).await
}

/// Like [`serve_http`], but takes an already-bound listener and an
/// arbitrary shutdown future instead of binding `addr` itself and always
/// waiting for a real OS signal. Binding separately is what lets a caller
/// (tests, mainly) discover an OS-assigned ephemeral port before serving on
/// it; a custom `shutdown` future is what lets anything - a test, another
/// signal source, a coordinating supervisor - trigger the same drain
/// behavior `serve_http` gets from `SIGTERM`/`SIGINT`.
pub async fn serve_http_on(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    listener: TcpListener,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let addr = listener.local_addr().ok();
    tracing::info!(
        "listening on http://{}",
        addr.map(|a| a.to_string()).unwrap_or_default()
    );

    let graceful = GracefulShutdown::new();
    let mut shutdown = std::pin::pin!(shutdown);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = app.clone();
                let container = container.clone();
                let watcher = graceful.watcher();
                tokio::spawn(async move {
                    let io = TokioIo::new(stream);
                    let builder = connection_builder();
                    let conn = builder.serve_connection(io, service_fn(move |req| handle(app.clone(), container.clone(), req)));
                    let conn = watcher.watch(conn);
                    if let Err(err) = conn.await {
                        tracing::debug!("connection error: {err}");
                    }
                });
            }
            () = shutdown.as_mut() => {
                tracing::info!("shutdown signal received, draining connections");
                break;
            }
        }
    }

    drain(graceful).await;
    Ok(())
}

/// Serves `app` over TLS (via rustls) on `addr`. `config` is typically
/// produced by [`load_tls`]. Same shutdown behavior as [`serve_http`].
pub async fn serve_https(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    addr: SocketAddr,
    config: TlsConfig,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    serve_https_on(app, container, listener, config, shutdown_signal()).await
}

/// Like [`serve_https`], but takes an already-bound listener and an
/// arbitrary shutdown future - see [`serve_http_on`]'s doc comment for why.
pub async fn serve_https_on(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    listener: TcpListener,
    config: TlsConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let addr = listener.local_addr().ok();
    tracing::info!(
        "listening on https://{}",
        addr.map(|a| a.to_string()).unwrap_or_default()
    );

    let graceful = GracefulShutdown::new();
    let mut shutdown = std::pin::pin!(shutdown);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let app = app.clone();
                let container = container.clone();
                let acceptor = acceptor.clone();
                let watcher = graceful.watcher();
                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(err) => {
                            tracing::debug!("TLS handshake failed: {err}");
                            return;
                        }
                    };
                    let io = TokioIo::new(tls_stream);
                    let builder = connection_builder();
                    let conn = builder.serve_connection(io, service_fn(move |req| handle(app.clone(), container.clone(), req)));
                    let conn = watcher.watch(conn);
                    if let Err(err) = conn.await {
                        tracing::debug!("connection error: {err}");
                    }
                });
            }
            () = shutdown.as_mut() => {
                tracing::info!("shutdown signal received, draining connections");
                break;
            }
        }
    }

    drain(graceful).await;
    Ok(())
}

/// Waits for every watched connection to finish, up to
/// [`DEFAULT_GRACEFUL_TIMEOUT`] - past which it gives up and returns
/// anyway, so a client that never closes its connection can't wedge
/// shutdown forever.
async fn drain(graceful: GracefulShutdown) {
    tokio::select! {
        () = graceful.shutdown() => {
            tracing::info!("all connections drained");
        }
        () = tokio::time::sleep(DEFAULT_GRACEFUL_TIMEOUT) => {
            tracing::warn!("graceful shutdown timed out after {DEFAULT_GRACEFUL_TIMEOUT:?}, forcing exit");
        }
    }
}

/// Resolves once a `SIGTERM` or `SIGINT` arrives (Unix), or Ctrl+C
/// (everywhere, including non-Unix). This is what a container orchestrator
/// sends before a harder kill - `docker stop`/`kubectl delete pod` both
/// signal `SIGTERM` first.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            std::future::pending::<()>().await;
            unreachable!();
        };
        sig.recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

async fn handle(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    req: http::Request<hyper::body::Incoming>,
) -> Result<http::Response<crate::response::BoxBody>, std::convert::Infallible> {
    let (parts, body) = req.into_parts();
    let request = Request::new(
        parts.method,
        parts.uri,
        parts.headers,
        InboundBody::from(body),
        container,
    );
    let response = app.call(request).await;
    Ok(response.into_hyper())
}

/// Loads a TLS server config from a PEM cert chain + private key, mirroring
/// what `quench-starter`'s actix bootstrap did - same file formats, same
/// `None` on missing/unreadable files so callers can fall back to plain
/// HTTP in dev.
pub fn load_tls(
    cert_path: impl AsRef<std::path::Path>,
    key_path: impl AsRef<std::path::Path>,
) -> Option<TlsConfig> {
    let cert_chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)
        .ok()?
        .collect::<Result<_, _>>()
        .ok()?;
    let key: PrivateKeyDer<'static> = PrivateKeyDer::from_pem_file(key_path).ok()?;

    TlsConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .ok()
}
