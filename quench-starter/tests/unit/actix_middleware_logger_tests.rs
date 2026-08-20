//! Unit tests for `actix/middleware/logger.rs`.
//!
//! `FilteredLogger` only decides *whether* to emit a log line - it never
//! changes the response - so these check that every response class (success,
//! skipped-prefix success, and error) still passes through unaltered while
//! exercising each branch of that decision.

use actix_web::{App, HttpResponse, get, test};
use quench_starter::actix::middleware::logger::FilteredLogger;

#[get("/ok")]
async fn ok() -> HttpResponse {
    HttpResponse::Ok().finish()
}

#[get("/boom")]
async fn boom() -> HttpResponse {
    HttpResponse::InternalServerError().finish()
}

#[actix_web::test]
async fn a_successful_response_passes_through_unchanged() {
    let app = test::init_service(App::new().wrap(FilteredLogger::default()).service(ok)).await;
    let response = test::call_service(&app, test::TestRequest::get().uri("/ok").to_request()).await;
    assert!(response.status().is_success());
}

#[actix_web::test]
async fn an_error_response_still_passes_through_unchanged() {
    let app = test::init_service(App::new().wrap(FilteredLogger::default()).service(boom)).await;
    let response =
        test::call_service(&app, test::TestRequest::get().uri("/boom").to_request()).await;
    assert!(response.status().is_server_error());
}

#[actix_web::test]
async fn a_path_under_a_skipped_prefix_is_still_served_normally() {
    // SAFETY: this test owns `LOG_SKIP_PREFIXES` for its duration and no other
    // test in this binary reads or writes it.
    unsafe {
        std::env::set_var("LOG_SKIP_PREFIXES", "/ok,/health");
    }
    let app = test::init_service(App::new().wrap(FilteredLogger::default()).service(ok)).await;
    let response = test::call_service(&app, test::TestRequest::get().uri("/ok").to_request()).await;
    assert!(response.status().is_success());
    unsafe {
        std::env::remove_var("LOG_SKIP_PREFIXES");
    }
}
