//! Unit tests for `actix/domain/error.rs`.

use actix_web::body::MessageBody;
use actix_web::http::StatusCode;
use quench_starter::actix::domain::error::{
    DENIED, UNAUTHORIZED, UNSUPPORTED, response, response_with_detail,
};
use serde_json::json;

async fn body_json(response: actix_web::HttpResponse) -> serde_json::Value {
    let bytes = response.into_body().try_into_bytes().unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn the_error_code_constants_are_the_expected_strings() {
    assert_eq!(UNAUTHORIZED, "UNAUTHORIZED");
    assert_eq!(DENIED, "DENIED");
    assert_eq!(UNSUPPORTED, "UNSUPPORTED");
}

#[actix_web::test]
async fn response_wraps_a_single_error_entry_with_an_empty_detail() {
    let res = response(StatusCode::UNAUTHORIZED, UNAUTHORIZED, "no token");
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({ "errors": [{ "code": "UNAUTHORIZED", "message": "no token", "detail": {} }] })
    );
}

#[actix_web::test]
async fn response_with_detail_carries_the_given_detail_payload() {
    let res = response_with_detail(
        StatusCode::FORBIDDEN,
        DENIED,
        "not in realm",
        json!({ "realm": "arda" }),
    );
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({ "errors": [{ "code": "DENIED", "message": "not in realm", "detail": { "realm": "arda" } }] })
    );
}
