//! `OnPathPrefix` is what lets a service scope a middleware (`Auth`, most
//! often) to part of its route tree, needed because
//! discovered routes are one flat router rather than per-scope middleware
//! stacks.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::middleware::{Middleware, OnPathPrefix, wrap};
use quench_http::request::Request;
use quench_http::response::Response;
use quench_proto::{HeaderMap, Method, StatusCode, Uri};
use std::sync::Arc;

struct AlwaysOk;

#[async_trait]
impl Endpoint for AlwaysOk {
    async fn call(&self, _req: Request) -> Response {
        Response::new(StatusCode::OK)
    }
}

struct RejectEverything;

#[async_trait]
impl Middleware for RejectEverything {
    async fn handle(&self, _req: Request, _next: &dyn Endpoint) -> Response {
        Response::new(StatusCode::FORBIDDEN)
    }
}

fn get(path: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[quench_test::test]
async fn middleware_runs_only_for_matching_paths() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(AlwaysOk);
    let app = wrap(app, OnPathPrefix::new("/api/", RejectEverything));

    let resp = app.call(get("/api/things", &container)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[quench_test::test]
async fn middleware_is_skipped_outside_the_prefix() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(AlwaysOk);
    let app = wrap(app, OnPathPrefix::new("/api/", RejectEverything));

    let resp = app.call(get("/health", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[quench_test::test]
async fn a_prefix_without_a_slash_stops_at_a_segment_boundary() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(AlwaysOk);
    let app = wrap(app, OnPathPrefix::new("/api", RejectEverything));

    for (path, status) in [
        ("/api", StatusCode::FORBIDDEN),
        ("/api/x", StatusCode::FORBIDDEN),
        ("/apix", StatusCode::OK),
    ] {
        let resp = app.call(get(path, &container)).await;
        assert_eq!(resp.status(), status, "{path}");
    }
}

quench_test::main!();
