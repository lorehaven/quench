//! Unit tests for `actix/routers/metrics.rs`.

use actix_web::{App, http::StatusCode, test};
use quench_starter::actix::routers::metrics::scope;

#[actix_web::test]
async fn metrics_endpoint_serves_a_prometheus_text_body() {
    let app = test::init_service(App::new().service(scope())).await;
    let response =
        test::call_service(&app, test::TestRequest::get().uri("/metrics").to_request()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_type.starts_with("text/plain"));
    let body = test::read_body(response).await;
    assert!(
        String::from_utf8(body.to_vec())
            .unwrap()
            .contains("service_up")
    );
}

#[actix_web::test]
async fn health_ready_and_live_endpoints_all_report_their_own_status() {
    let app = test::init_service(App::new().service(scope())).await;

    for (path, expected_status) in [
        ("/health", "healthy"),
        ("/health/ready", "ready"),
        ("/health/live", "alive"),
    ] {
        let response =
            test::call_service(&app, test::TestRequest::get().uri(path).to_request()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["status"], expected_status);
    }
}
