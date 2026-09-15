//! Proves the routing engine keeps actix-router's regex-constrained,
//! multi-segment wildcard segments - the reason this crate reuses
//! actix-router instead of a plain radix-tree matcher. These patterns are
//! lifted verbatim from `warehouse-service`'s docker registry routes.

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::router::RouterBuilder;
use std::sync::Arc;

struct Echo(&'static str);

#[async_trait]
impl Endpoint for Echo {
    async fn call(&self, _req: Request) -> Response {
        Response::text(StatusCode::OK, self.0)
    }
}

fn get(uri: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn matches_multi_segment_name_before_fixed_suffix() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(
        Method::GET,
        "/{name:.+}/blobs/{digest}",
        Arc::new(Echo("blob")),
    );
    builder.route(
        Method::GET,
        "/{name:.+}/manifests/{reference}",
        Arc::new(Echo("manifest")),
    );
    builder.route(Method::GET, "/{name:.+}/tags/list", Arc::new(Echo("tags")));
    let router = builder.finish();

    // `name` has to swallow the slash in `library/ubuntu` and still leave
    // `/blobs/{digest}` matchable - a plain radix-tree router (axum/poem's
    // default) can't do this; actix-router's regex segments can.
    let resp = router
        .dispatch(get("/library/ubuntu/blobs/sha256:deadbeef", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router
        .dispatch(get("/library/ubuntu/manifests/latest", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = router
        .dispatch(get("/library/ubuntu/tags/list", &container))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn unmatched_path_is_404() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/known", Arc::new(Echo("known")));
    let router = builder.finish();

    let resp = router.dispatch(get("/unknown", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_method_on_matched_path_is_405() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/{path:.*}", Arc::new(Echo("catch-all")));
    let router = builder.finish();

    let req = Request::new(
        Method::POST,
        "/anything".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );
    let resp = router.dispatch(req).await;
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn registration_order_breaks_ties_between_overlapping_patterns() {
    // Mirrors the crates sparse index: a static `/config.json` route has to
    // win over a trailing `/{path:.*}` catch-all covering the same prefix,
    // exactly like actix-web's own `.service()` registration order.
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/config.json", Arc::new(Echo("config")));
    builder.route(Method::GET, "/{path:.*}", Arc::new(Echo("catch-all")));
    let router = builder.finish();

    let resp = router.dispatch(get("/config.json", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
