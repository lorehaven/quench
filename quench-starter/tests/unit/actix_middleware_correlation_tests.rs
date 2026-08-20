//! Unit tests for `actix/middleware/correlation.rs`.

use actix_web::{App, HttpMessage, HttpResponse, get, test};
use quench_starter::actix::middleware::correlation::{
    CorrelationIdMiddleware, get_correlation_id, inject_correlation_id_header,
};

#[get("/echo")]
async fn echo(req: actix_web::HttpRequest) -> HttpResponse {
    let id = get_correlation_id(&req.extensions());
    HttpResponse::Ok().body(id)
}

#[actix_web::test]
async fn a_request_with_no_correlation_id_gets_one_generated_and_echoed_back() {
    let app = test::init_service(App::new().wrap(CorrelationIdMiddleware).service(echo)).await;

    let response =
        test::call_service(&app, test::TestRequest::get().uri("/echo").to_request()).await;
    assert!(response.status().is_success());

    let header = response
        .headers()
        .get("X-Correlation-ID")
        .expect("a generated correlation id header")
        .to_str()
        .expect("a header that is text")
        .to_string();
    assert_eq!(header.len(), 36, "a UUID string: {header}");

    let body = test::read_body(response).await;
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), header);
}

#[actix_web::test]
async fn a_request_with_an_existing_correlation_id_propagates_it_unchanged() {
    let app = test::init_service(App::new().wrap(CorrelationIdMiddleware).service(echo)).await;

    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/echo")
            .insert_header(("X-Correlation-ID", "fixed-id-123"))
            .to_request(),
    )
    .await;

    let header = response
        .headers()
        .get("X-Correlation-ID")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(header, "fixed-id-123");

    let body = test::read_body(response).await;
    assert_eq!(String::from_utf8(body.to_vec()).unwrap(), "fixed-id-123");
}

#[actix_web::test]
async fn get_correlation_id_falls_back_to_unknown_when_nothing_was_stored() {
    let extensions = actix_web::dev::Extensions::new();
    assert_eq!(get_correlation_id(&extensions), "unknown");
}

#[actix_web::test]
async fn inject_correlation_id_header_adds_the_header_to_an_outgoing_request() {
    let client = reqwest::Client::new();
    let request = inject_correlation_id_header(client.get("http://example.invalid"), "abc-123")
        .build()
        .expect("a built request");
    assert_eq!(
        request.headers().get("X-Correlation-ID").unwrap(),
        "abc-123"
    );
}
