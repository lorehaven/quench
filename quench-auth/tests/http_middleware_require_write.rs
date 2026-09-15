//! Mirrors the coverage intent of the actix `RequireWrite` tests: safe
//! methods always pass, a write needs the permission, and mounting order
//! (`Auth` outermost) is what makes `Claims` visible to this middleware.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::middleware::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "sage".to_string();
    config.audiences = vec!["sage".to_string()];
    config.auth_enabled = true;
    config
}

struct Ok200;

#[async_trait::async_trait]
impl Endpoint for Ok200 {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

fn app(config: JwtConfig) -> Arc<dyn Endpoint> {
    let inner: Arc<dyn Endpoint> = Arc::new(Ok200);
    let inner = wrap(inner, RequireWrite::new(config.clone()));
    // Auth has to be outermost so its Claims exist by the time RequireWrite
    // runs - see `quench_auth::http::middleware`'s doc comment.
    wrap(inner, Auth::new(config))
}

fn request(
    method: Method,
    container: &Arc<quench_http::di::Container>,
    token: Option<&str>,
) -> Request {
    let mut headers = HeaderMap::new();
    if let Some(token) = token {
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    Request::new(
        method,
        "/x".parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn a_get_never_needs_write_even_with_no_token() {
    let app = app(config());
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    // GET with no token still 401s from `Auth` itself - `RequireWrite`'s
    // safe-method bypass doesn't skip authentication, only the permission
    // check.
    let resp = app.call(request(Method::GET, &container, None)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_post_with_a_read_only_token_is_forbidden() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "sage:read".to_string(), None)
        .await
        .unwrap();
    let app = app(cfg);
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app
        .call(request(Method::POST, &container, Some(&token)))
        .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_post_with_a_write_token_is_allowed() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "sage:write".to_string(), None)
        .await
        .unwrap();
    let app = app(cfg);
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app
        .call(request(Method::POST, &container, Some(&token)))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_post_with_an_admin_wildcard_token_is_allowed() {
    let cfg = config();
    let token = cfg
        .issue_access_token("root".to_string(), "admin".to_string(), None)
        .await
        .unwrap();
    let app = app(cfg);
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app
        .call(request(Method::POST, &container, Some(&token)))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
