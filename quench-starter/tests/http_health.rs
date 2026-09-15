//! End-to-end smoke test for the quench-http bootstrap's health/readiness
//! routes and base-path mounting - the same wiring `http::serve` does,
//! minus actually opening a socket.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::prelude::{ContainerBuilder, Endpoint, discover_routes, mount};
use quench_http::request::Request;
use quench_starter::common::health::HealthState;
use std::sync::Arc;

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
async fn health_live_and_ready_reflect_state_and_swagger_redirects() {
    let health_state = HealthState::live();
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(health_state.clone())
            .build()
            .await
            .unwrap(),
    );

    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let app = mount("/svc", router);

    let resp = app.call(get("/svc/health", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Not ready yet.
    let resp = app.call(get("/svc/health/ready", &container)).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    health_state.mark_ready();
    let resp = app.call(get("/svc/health/ready", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.call(get("/svc/health/live", &container)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.call(get("/svc/swagger-ui", &container)).await;
    assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);

    // Outside the mounted base path.
    let resp = app.call(get("/health", &container)).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
