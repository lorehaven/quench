//! `mount` is what lets a service's whole route tree sit under a
//! runtime-configured `BASE_PATH` (an env var, so it can't be baked into
//! the `&'static str` route patterns `#[get]`/... register).

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::router::{RouterBuilder, mount};
use quench_proto::{HeaderMap, Method, StatusCode, Uri};
use std::sync::Arc;

struct Echo;

#[quench_core::async_trait]
impl Endpoint for Echo {
    async fn call(&self, _req: Request) -> Response {
        Response::text(StatusCode::OK, "ok")
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

#[quench_test::test]
async fn requests_inside_the_prefix_are_dispatched_with_it_stripped() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/widgets/{id}", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/api", router);

    let resp = mounted.call(get("/api/widgets/42", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[quench_test::test]
async fn requests_outside_the_prefix_404_before_reaching_inner_routes() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/widgets/{id}", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/api", router);

    let resp = mounted.call(get("/widgets/42", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn prefix_match_is_segment_aware_not_a_bare_string_prefix() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/thing", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/api", router);

    // `/api-legacy/thing` must not match a mount at `/api`.
    let resp = mounted.call(get("/api-legacy/thing", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn empty_or_root_prefix_mounts_at_the_root_unchanged() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/thing", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/", router);

    let resp = mounted.call(get("/thing", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[quench_test::test]
async fn mounts_nest_and_a_trailing_slash_on_the_prefix_is_ignored() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/x", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());

    let nested = mount("/a", mount("/b", router.clone()));
    let resp = nested.call(get("/a/b/x", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let slashed = mount("/api/", router);
    let resp = slashed.call(get("/api/x", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

quench_test::main!();
