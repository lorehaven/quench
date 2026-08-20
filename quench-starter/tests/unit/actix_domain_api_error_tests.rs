//! Unit tests for `actix/domain/api_error.rs`.

use actix_web::body::MessageBody;
use actix_web::http::StatusCode;
use quench_starter::actix::domain::api_error::{ApiError, json_error};

async fn body_string(response: actix_web::HttpResponse) -> String {
    let bytes = response.into_body().try_into_bytes().unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[actix_web::test]
async fn json_error_wraps_the_message_in_an_error_field() {
    let response = json_error(StatusCode::BAD_REQUEST, "bad input");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_string(response).await, r#"{"error":"bad input"}"#);
}

#[actix_web::test]
async fn a_server_error_api_error_still_renders_as_json() {
    let response = ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "db down").into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body_string(response).await, r#"{"error":"db down"}"#);
}

#[actix_web::test]
async fn a_client_error_api_error_renders_as_json_without_panicking() {
    let response = ApiError::new(StatusCode::NOT_FOUND, "missing").into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_string(response).await, r#"{"error":"missing"}"#);
}
