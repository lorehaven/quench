//! quench-http counterpart to `crate::actix` - same bootstrap shape
//! (base-path scope, health/readiness, swagger redirect, UI base-path/root
//! redirects, filtered logger + correlation-id middleware, TLS with an
//! HTTP->HTTPS redirect server), rebuilt on `quench_http` instead of
//! `actix-web`.

pub mod domain;
pub mod middleware;
pub mod routers;

use crate::common::db::DbWrapper;
use crate::common::health::HealthState;
use crate::common::routes::normalize_base_path;
use async_trait::async_trait;
use quench_cli::prelude::{Tone, print_status};
use quench_http::prelude::http::{Method, StatusCode, uri::Authority};
use quench_http::prelude::*;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

/// Boots a service on quench-http. Builds the dependency graph (every
/// `#[injectable]` linked into the binary, plus `Db` and `HealthState`
/// seeded directly since they're constructed before the graph exists),
/// discovers every `#[get]`/`#[post]`/... route, mounts the whole thing
/// under `BASE_PATH`, and serves it.
pub async fn serve<I>(db: Option<Arc<DbWrapper>>, init: I) -> std::io::Result<()>
where
    I: Future<Output = ()> + Send + 'static,
{
    let _ = rustls::crypto::ring::default_provider().install_default();

    let base_path = normalize_base_path(&envmnt::get_or("BASE_PATH", "/"));
    tracing::info!("Server initialized with BASE_PATH: {base_path}");

    let db_wrapper = match db {
        Some(d) => d,
        None => DbWrapper::init_env().await,
    };
    let health_state = HealthState::live();

    let init_health_state = health_state.clone();
    tokio::spawn(async move {
        init.await;
        init_health_state.mark_ready();
        tracing::info!("Service initialization complete");
    });

    let container = ContainerBuilder::new()
        .provide(db_wrapper.db.clone())
        .provide(health_state)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    let app = discover_and_mount(base_path);

    let (https_addr, http_addr) = server_addrs();
    let service_name = envmnt::get_or("SERVICE_NAME", "service");

    let tls = load_tls(
        envmnt::get_or("SERVER_CERT_PATH", "cert.pem"),
        envmnt::get_or("SERVER_KEY_PATH", "key.pem"),
    );

    match tls {
        Some(config) => {
            print_status(
                Tone::Success,
                &format!("{service_name}-service"),
                &format!("starting HTTPS server on {https_addr}"),
            );
            print_status(
                Tone::Info,
                &format!("{service_name}-service"),
                &format!("starting HTTP redirect server on {http_addr}"),
            );

            let redirect: Arc<dyn Endpoint> = Arc::new(RedirectToHttps {
                https_port: https_addr.port(),
            });

            let https = serve_https(app, container.clone(), https_addr, config);
            let redirect = serve_http(redirect, container, http_addr);
            tokio::try_join!(https, redirect)?;
            Ok(())
        }
        None => {
            print_status(
                Tone::Warn,
                &format!("{service_name}-service"),
                "starting plain HTTP server",
            );
            serve_http(app, container, https_addr).await
        }
    }
}

/// The composable half of [`serve`]: discovers every `#[get]`/`#[post]`/...
/// route linked into the binary (plus `quench-starter`'s own
/// health/swagger/UI-redirect routes), mounts the whole thing under
/// `base_path`, and wraps it in the correlation-id and filtered-logger
/// middleware - everything [`serve`] does *except* opening a socket and
/// building the DI container, so a service that needs its own middleware
/// (auth, most often) around this can add it before ever calling
/// `serve_http`/`serve_https` itself:
///
/// ```ignore
/// let app = quench_starter::http::discover_and_mount(base_path);
/// let app = wrap(app, RequireWrite::new(config.clone()));
/// let app = wrap(app, Auth::new(config)); // outermost: runs first
/// serve_http(app, container, addr).await
/// ```
///
/// (See `quench_auth::http::middleware`'s doc comment for why `Auth` has
/// to be the outermost layer there.) [`serve`] itself calls this with no
/// middleware added, for the common case that doesn't need any.
pub fn discover_and_mount(base_path: impl Into<String>) -> Arc<dyn Endpoint> {
    let base_path = base_path.into();

    // Guarantees `health`/`swagger`/`base_path_redirect`/`base_path_slash_redirect`
    // are linked into this binary before `discover_routes()` runs - see
    // `routers::swagger::force_link`'s doc comment for why that isn't
    // automatic just because `quench-starter` is a dependency.
    routers::health::force_link();
    routers::swagger::force_link();
    routers::ui::force_link();

    // Before the discovered route set: every service mounts its own routes
    // under a catch-all pattern, which would otherwise swallow the bare
    // base path and 404 before `base_path_redirect`/`base_path_slash_redirect`
    // (both `#[get]`-discovered, like health/swagger) ever got a look.
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let mounted = mount(base_path.clone(), router);
    let app: Arc<dyn Endpoint> = Arc::new(RootOrMounted { base_path, mounted });
    let app = wrap(app, middleware::correlation::CorrelationId);
    wrap(app, middleware::logger::FilteredLogger::default())
}

fn server_addrs() -> (SocketAddr, SocketAddr) {
    let addr_str: String = envmnt::get_or("SERVER_ADDR", "0.0.0.0:443");
    let addr_redir_str: String = envmnt::get_or("SERVER_HTTP_REDIRECT_ADDR", "0.0.0.0:80");

    let https_addr: SocketAddr = addr_str
        .parse()
        .expect("SERVER_ADDR must be a valid socket address");
    let http_addr: SocketAddr = addr_redir_str
        .parse()
        .expect("SERVER_HTTP_REDIRECT_ADDR must be a valid socket address");
    (https_addr, http_addr)
}

/// Answers the bare server root (`GET /`) with `routers::ui::server_root_redirect`
/// when `BASE_PATH` isn't `/` - a request that doesn't start with
/// `BASE_PATH` never reaches `mounted` at all, so this has to sit outside
/// it. When `BASE_PATH` *is* `/`, `mount` already delegates every request
/// straight through unchanged, and the mounted router's own
/// `base_path_slash_redirect` (pattern `"/"`) answers `GET /` the same way,
/// so this struct only needs to special-case the non-trivial-base-path
/// case. See `routers::ui`'s module doc comment for why these can't share
/// one discovered router.
struct RootOrMounted {
    base_path: String,
    mounted: Arc<dyn Endpoint>,
}

#[async_trait]
impl Endpoint for RootOrMounted {
    async fn call(&self, req: Request) -> Response {
        if self.base_path != "/" && req.method() == Method::GET && req.uri().path() == "/" {
            return routers::ui::server_root_redirect().await;
        }
        self.mounted.call(req).await
    }
}

struct RedirectToHttps {
    https_port: u16,
}

#[async_trait]
impl Endpoint for RedirectToHttps {
    async fn call(&self, req: Request) -> Response {
        // Reads the `Host` header directly rather than actix's
        // `connection_info()` (which also honors `X-Forwarded-Host`) - fine
        // for a redirect server sitting directly in front of the HTTPS
        // listener; add forwarded-header handling here if this ever sits
        // behind another proxy.
        let host = req.header("host").unwrap_or("").to_string();
        let authority = build_https_authority(&host, self.https_port);
        let location = format!("https://{authority}{}", req.uri());
        Response::new(StatusCode::PERMANENT_REDIRECT).header("Location", location)
    }
}

fn build_https_authority(host: &str, https_port: u16) -> String {
    if let Ok(authority) = host.parse::<Authority>() {
        let parsed_host = authority.host();
        let rendered_host = if parsed_host.contains(':') {
            format!("[{parsed_host}]")
        } else {
            parsed_host.to_string()
        };

        if https_port == 443 {
            rendered_host
        } else {
            format!("{rendered_host}:{https_port}")
        }
    } else if https_port == 443 {
        host.to_string()
    } else {
        format!("{host}:{https_port}")
    }
}

#[cfg(test)]
mod root_or_mounted_tests {
    use super::*;
    use bytes::Bytes;
    use quench_http::body::InboundBody;
    use quench_http::di::ContainerBuilder;
    use quench_http::prelude::http::{HeaderMap, Uri};

    struct AlwaysOk;

    #[async_trait]
    impl Endpoint for AlwaysOk {
        async fn call(&self, _req: Request) -> Response {
            Response::new(StatusCode::OK)
        }
    }

    async fn request(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
        Request::new(
            Method::GET,
            path.parse::<Uri>().unwrap(),
            HeaderMap::new(),
            InboundBody::from_bytes(Bytes::new()),
            container.clone(),
        )
    }

    #[tokio::test]
    async fn bare_root_redirects_to_ui_when_base_path_is_not_root() {
        // Not asserting the exact `Location` value: `routers::ui::ui_path`
        // reads `BASE_PATH` from a process-global `LazyLock` (see
        // `common::routes`), independent of the `base_path` field this
        // struct was built with, and that static is whatever the *first*
        // caller in this test binary happened to initialize it to. What
        // this test actually verifies - that a non-root base path makes
        // `RootOrMounted` intercept the bare root instead of forwarding it
        // to `mounted` - doesn't depend on that value.
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        let app = RootOrMounted {
            base_path: "/svc".to_string(),
            mounted: Arc::new(AlwaysOk),
        };

        let resp = app.call(request("/", &container).await).await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let hyper_resp = resp.into_hyper();
        assert!(hyper_resp.headers().contains_key("location"));
    }

    #[tokio::test]
    async fn anything_other_than_bare_root_falls_through_to_mounted() {
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        let app = RootOrMounted {
            base_path: "/svc".to_string(),
            mounted: Arc::new(AlwaysOk),
        };

        let resp = app.call(request("/svc/health", &container).await).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn root_base_path_never_intercepts_leaves_it_to_mounted() {
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        let app = RootOrMounted {
            base_path: "/".to_string(),
            mounted: Arc::new(AlwaysOk),
        };

        let resp = app.call(request("/", &container).await).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
