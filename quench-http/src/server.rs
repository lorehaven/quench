use crate::body::InboundBody;
use crate::di::Container;
use crate::endpoint::Endpoint;
use crate::request::Request;
use quench_core::future::BoxFuture;
use quench_proto::h1::server::{
    ServerConfig as ConnConfig, Service, Shutdown, serve_connection, shutdown_channel,
};
use quench_proto::io::Io;
#[cfg(feature = "tls")]
use quench_proto::tls::TlsAcceptor as Acceptor;
#[cfg(feature = "tls")]
use rustls::ServerConfig as TlsConfig;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// Stands in for the TLS acceptor when the `tls` feature is off: no value of it can exist, so the
/// server loop's TLS branch compiles away without a second code path.
#[cfg(not(feature = "tls"))]
#[derive(Clone)]
enum Acceptor {}

/// How long [`serve_http`]/[`serve_https`] wait, after a shutdown signal,
/// for in-flight connections to drain on their own before forcing an exit.
///
/// Kept comfortably under Kubernetes' default 30s
/// `terminationGracePeriodSeconds`, so the process exits on its own instead
/// of being `SIGKILLed` mid-drain.
pub const DEFAULT_GRACEFUL_TIMEOUT: Duration = Duration::from_secs(25);

/// How long a connection may take sending its request headers before it's
/// dropped.
///
/// Without this, a client that opens a connection and then sends
/// headers one byte at a time (or never finishes them) ties up a
/// connection slot forever - the "slowloris" class of resource-exhaustion
/// attack. Generous enough for a slow mobile client, tight enough that a
/// deliberately stalled connection can't accumulate.
pub const DEFAULT_HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Serves `app` over plain HTTP/1.1 on `addr`.
///
/// Each connection is handled on its own task and may pipeline and reuse the
/// connection. Runs until a `SIGTERM`/`SIGINT` (or, on non-Unix,
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
/// waiting for a real OS signal.
///
/// Binding separately is what lets a caller
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
    serve_loop(app, container, listener, None, shutdown).await
}

/// Serves `app` over TLS (via rustls) on `addr`. `config` is typically
/// produced by [`load_tls`]. Same shutdown behavior as [`serve_http`].
#[cfg(feature = "tls")]
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
#[cfg(feature = "tls")]
pub async fn serve_https_on(
    app: Arc<dyn Endpoint>,
    container: Arc<Container>,
    listener: TcpListener,
    config: TlsConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    serve_loop(
        app,
        container,
        listener,
        Some(Acceptor::from(config)),
        shutdown,
    )
    .await
}

/// Adapts an [`Endpoint`] to the protocol layer's [`Service`].
struct App {
    endpoint: Arc<dyn Endpoint>,
    container: Arc<Container>,
}

impl Service for App {
    fn call(&self, req: quench_proto::Request) -> BoxFuture<'_, quench_proto::Response> {
        Box::pin(async move {
            let quench_proto::Request {
                method,
                uri,
                headers,
                body,
                ..
            } = req;
            let request = Request::new(
                method,
                uri,
                headers,
                InboundBody::from(body),
                self.container.clone(),
            );
            self.endpoint.call(request).await.into_inner()
        })
    }
}

async fn serve_loop(
    endpoint: Arc<dyn Endpoint>,
    container: Arc<Container>,
    listener: TcpListener,
    tls: Option<Acceptor>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let addr = listener
        .local_addr()
        .ok()
        .map(|a| a.to_string())
        .unwrap_or_default();
    quench_log::info!(
        "listening on {}://{addr}",
        if tls.is_some() { "https" } else { "http" }
    );

    let config = Arc::new(ConnConfig {
        header_read_timeout: DEFAULT_HEADER_READ_TIMEOUT,
        ..ConnConfig::default()
    });
    let app = Arc::new(App {
        endpoint,
        container,
    });
    let (signal, conn_shutdown) = shutdown_channel();
    // Each connection task holds a sender; the channel closes once the last one ends.
    let (done_tx, mut done_rx) = mpsc::channel::<()>(1);
    let mut shutdown = std::pin::pin!(shutdown);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let stream = match accepted {
                    Ok((stream, _)) => stream,
                    Err(err) => {
                        quench_log::warn!("accept failed: {err}");
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }
                };
                let _ = stream.set_nodelay(true);
                let task = connection(stream, tls.clone(), app.clone(), config.clone(), conn_shutdown.clone());
                let done = done_tx.clone();
                tokio::spawn(async move {
                    task.await;
                    drop(done);
                });
            }
            () = shutdown.as_mut() => {
                quench_log::info!("shutdown signal received, draining connections");
                break;
            }
        }
    }

    signal.signal();
    drop(done_tx);
    tokio::select! {
        _ = done_rx.recv() => quench_log::info!("all connections drained"),
        () = tokio::time::sleep(DEFAULT_GRACEFUL_TIMEOUT) => {
            quench_log::warn!("graceful shutdown timed out after {DEFAULT_GRACEFUL_TIMEOUT:?}, forcing exit");
        }
    }
    Ok(())
}

async fn connection(
    stream: tokio::net::TcpStream,
    tls: Option<Acceptor>,
    app: Arc<App>,
    config: Arc<ConnConfig>,
    shutdown: Shutdown,
) {
    match tls {
        None => run(stream, &app, &config, shutdown).await,
        #[cfg(feature = "tls")]
        Some(acceptor) => {
            match tokio::time::timeout(config.header_read_timeout, acceptor.accept(stream)).await {
                Ok(Ok(stream)) => run(stream, &app, &config, shutdown).await,
                Ok(Err(err)) => quench_log::debug!("TLS handshake failed: {err}"),
                Err(_) => quench_log::debug!("TLS handshake timed out"),
            }
        }
        #[cfg(not(feature = "tls"))]
        Some(never) => match never {},
    }
}

async fn run<I: Io>(io: I, app: &App, config: &ConnConfig, shutdown: Shutdown) {
    if let Err(err) = serve_connection(io, app, config, shutdown).await {
        quench_log::debug!("connection error: {err}");
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

/// Loads a TLS server config from a PEM cert chain + private key, returning
/// `None` on missing/unreadable files so callers can fall back to plain
/// HTTP in dev.
#[cfg(feature = "tls")]
pub fn load_tls(
    cert_path: impl AsRef<std::path::Path>,
    key_path: impl AsRef<std::path::Path>,
) -> Option<TlsConfig> {
    let mut config = quench_proto::tls::load_server_config(cert_path, key_path)?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Some(config)
}
