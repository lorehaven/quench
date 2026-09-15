//! `mount` is what lets a service's whole route tree sit under a
//! runtime-configured `BASE_PATH` (an env var, so it can't be baked into
//! the `&'static str` route patterns `#[get]`/... register) - the
//! quench-http equivalent of actix-web's `web::scope(&base_path)`.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::router::{RouterBuilder, mount};
use std::sync::Arc;

struct Echo;

#[async_trait::async_trait]
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

#[tokio::test]
async fn requests_inside_the_prefix_are_dispatched_with_it_stripped() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/widgets/{id}", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/api", router);

    let resp = mounted.call(get("/api/widgets/42", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn requests_outside_the_prefix_404_before_reaching_inner_routes() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/widgets/{id}", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/api", router);

    let resp = mounted.call(get("/widgets/42", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
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

#[tokio::test]
async fn empty_or_root_prefix_mounts_at_the_root_unchanged() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let mut builder = RouterBuilder::default();
    builder.route(Method::GET, "/thing", Arc::new(Echo));
    let router: Arc<dyn Endpoint> = Arc::new(builder.finish());
    let mounted = mount("/", router);

    let resp = mounted.call(get("/thing", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
