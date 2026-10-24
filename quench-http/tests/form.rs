//! `Form<T>` reads `application/x-www-form-urlencoded` bodies - what an
//! HTML `<form method="post">` (no JS/htmx) sends, and what
//! `switchboard-service`'s `delete_model_form` needs.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::bytes::Bytes as RawBytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::extract::{Form, FromRequest};
use quench_http::request::Request;
use quench_proto::{HeaderMap, Method, StatusCode, Uri};
use quench_serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct DeleteRequest {
    path: String,
}

fn form_request(body: &'static str, container: &Arc<quench_http::di::Container>) -> Request {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    Request::new(
        Method::POST,
        "/".parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(RawBytes::from_static(body.as_bytes())),
        container.clone(),
    )
}

#[quench_test::test]
async fn decodes_a_url_encoded_body() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = form_request("path=%2Fmnt%2Fdev%2Fhuggingface%2Fhub%2Fmodel", &container);

    let Form(value) = Form::<DeleteRequest>::from_request(&mut req)
        .await
        .expect("valid form body");
    assert_eq!(value.path, "/mnt/dev/huggingface/hub/model");
}

#[quench_test::test]
async fn malformed_body_is_a_bad_request() {
    // Missing the required `path` field entirely.
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = form_request("unrelated=1", &container);

    let Err(err) = Form::<DeleteRequest>::from_request(&mut req).await else {
        panic!("expected a missing required field to fail");
    };
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
}

quench_test::main!();
