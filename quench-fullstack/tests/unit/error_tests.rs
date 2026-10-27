//! How errors map to statuses and bodies, and back.

use quench_fullstack::{ErrorBody, FieldError, ServerError};
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{ExtractError, HttpError};

fn all() -> Vec<ServerError> {
    vec![
        ServerError::unauthorized("sign in"),
        ServerError::forbidden("not yours"),
        ServerError::not_found("no such note"),
        ServerError::invalid("title", "too short"),
        ServerError::Internal("boom".into()),
        ServerError::Network("offline".into()),
        ServerError::BadResponse("odd".into()),
    ]
}

#[quench_test::test]
fn each_variant_has_a_status_a_kind_and_a_message() {
    let expected = [
        (401, "unauthorized"),
        (403, "forbidden"),
        (404, "not_found"),
        (422, "validation"),
        (500, "internal"),
        (502, "network"),
        (502, "bad_response"),
    ];
    for (error, (status, kind)) in all().iter().zip(expected) {
        assert_eq!((error.status(), error.kind()), (status, kind));
        assert!(!error.message().is_empty());
        assert!(error.to_string().starts_with(kind));
    }
    assert_eq!(
        ServerError::invalid("title", "too short").message(),
        "title: too short"
    );
    assert!(ServerError::not_found("x").fields().is_empty());
    let _: &dyn std::error::Error = &ServerError::not_found("x");
}

#[quench_test::test]
fn the_server_errors_survive_the_wire() {
    for error in all().into_iter().take(4) {
        let body = quench_serde::json::to_string(&error.to_body());
        assert_eq!(
            ServerError::from_response(error.status(), &body),
            error,
            "{body}"
        );
    }
}

#[quench_test::test]
fn an_internal_error_is_described_generically() {
    let body = ServerError::internal("password=hunter2 refused by db-7").to_body();
    assert_eq!(body.kind, "internal");
    assert_eq!(body.message, "internal server error");
    // And what the browser makes of it keeps that wording.
    let json = quench_serde::json::to_string(&body);
    assert_eq!(
        ServerError::from_response(500, &json),
        ServerError::Internal("internal server error".into())
    );
}

#[quench_test::test]
fn a_response_that_is_not_ours_falls_back_on_the_status() {
    assert_eq!(
        ServerError::from_response(401, ""),
        ServerError::Unauthorized("the server answered 401".into())
    );
    assert_eq!(
        ServerError::from_response(403, "<html>nope</html>"),
        ServerError::Forbidden("the server answered 403".into())
    );
    assert_eq!(
        ServerError::from_response(404, "not found"),
        ServerError::NotFound("the server answered 404".into())
    );
    assert_eq!(ServerError::from_response(422, "{}").kind(), "validation");
    assert_eq!(ServerError::from_response(500, "oops").kind(), "internal");
    for status in [400, 418, 502, 503] {
        assert_eq!(
            ServerError::from_response(status, "<h1>bad gateway</h1>").kind(),
            "bad_response",
            "{status}"
        );
    }
    // A body that names a kind decides, whatever the status.
    let body = quench_serde::json::to_string(&ErrorBody {
        kind: "not_found".into(),
        message: "gone".into(),
        fields: vec![],
    });
    assert_eq!(
        ServerError::from_response(400, &body),
        ServerError::NotFound("gone".into())
    );
}

#[quench_test::test]
fn fields_are_kept_for_validation_only() {
    let error = ServerError::Validation {
        message: "bad".into(),
        fields: vec![FieldError {
            field: "a".into(),
            message: "x".into(),
        }],
    };
    assert_eq!(error.to_body().fields.len(), 1);
    assert!(ServerError::forbidden("x").to_body().fields.is_empty());
}

#[quench_test::test]
async fn the_response_carries_the_status_the_body_and_no_caching() {
    for error in all().into_iter().take(5) {
        let response = error.clone().into_response().into_inner();
        assert_eq!(response.status.as_u16(), error.status());
        assert_eq!(
            response
                .headers
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        assert_eq!(
            response
                .headers
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap(),
            "no-store"
        );
        let body = response.body.collect_limited(1 << 16).await.unwrap();
        let parsed: ErrorBody = quench_serde::json::from_slice(&body).unwrap();
        assert_eq!(parsed.kind, error.kind());
    }
}

#[quench_test::test]
fn extractor_failures_map_to_the_matching_variant() {
    let status = |code: StatusCode| ServerError::from_http(&HttpError::status(code, "m"));
    assert_eq!(
        status(StatusCode::UNAUTHORIZED),
        ServerError::Unauthorized("m".into())
    );
    assert_eq!(
        status(StatusCode::FORBIDDEN),
        ServerError::Forbidden("m".into())
    );
    assert_eq!(
        status(StatusCode::NOT_FOUND),
        ServerError::NotFound("m".into())
    );
    assert_eq!(status(StatusCode::BAD_REQUEST).kind(), "validation");
    assert_eq!(status(StatusCode::PAYLOAD_TOO_LARGE).kind(), "validation");
    assert_eq!(
        status(StatusCode::INTERNAL_SERVER_ERROR),
        ServerError::Internal("m".into())
    );
    assert_eq!(
        status(StatusCode::BAD_GATEWAY),
        ServerError::Internal("m".into())
    );
    let extraction = ServerError::from_http(&HttpError::Extraction(ExtractError::InvalidJson(
        "eof".into(),
    )));
    assert_eq!(extraction.kind(), "validation");
    assert!(extraction.message().contains("eof"));
}
