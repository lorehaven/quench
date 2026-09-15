//! Proves `BodyLimit` (the `web::PayloadConfig`-equivalent knob warehouse
//! needs for large uploads) actually changes what `Json`/`Bytes` accept,
//! and that `Limited<_, N>` can override it per-route without touching the
//! app-wide default.

use bytes::Bytes as RawBytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::extract::{BodyLimit, Bytes, FromRequest, Json, Limited};
use quench_http::request::Request;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct Small {
    ok: bool,
}

fn request_with_body(body: &'static str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::POST,
        "/".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(RawBytes::from_static(body.as_bytes())),
        container.clone(),
    )
}

#[tokio::test]
async fn json_uses_default_limit_when_nothing_was_provided() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut req = request_with_body(r#"{"ok":true}"#, &container);

    let Json(value) = Json::<Small>::from_request(&mut req)
        .await
        .expect("well within the 2MB default");
    assert!(value.ok);
}

#[tokio::test]
async fn provided_body_limit_rejects_a_body_over_the_configured_cap() {
    // A limit smaller than the body itself - proves the *provided* value is
    // what's actually consulted, not just the compiled-in default.
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(5))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request_with_body(r#"{"ok":true}"#, &container);

    let err = match Json::<Small>::from_request(&mut req).await {
        Err(e) => e,
        Ok(_) => panic!("expected the body to exceed the 5-byte limit"),
    };
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn provided_body_limit_accepts_a_body_within_the_configured_cap() {
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(1024))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request_with_body(r#"{"ok":true}"#, &container);

    let Json(value) = Json::<Small>::from_request(&mut req)
        .await
        .expect("within the provided 1KB limit");
    assert!(value.ok);
}

#[tokio::test]
async fn bytes_extractor_respects_the_same_body_limit() {
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(3))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request_with_body("hello", &container);

    let err = match Bytes::from_request(&mut req).await {
        Err(e) => e,
        Ok(_) => panic!("expected the body to exceed the 3-byte limit"),
    };
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn limited_overrides_the_app_wide_body_limit_for_one_route() {
    // App-wide limit is tiny, but this route asks for more via `Limited`.
    let container = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(1))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request_with_body(r#"{"ok":true}"#, &container);

    let Limited(Json(value)) = Limited::<Json<Small>, 1024>::from_request(&mut req)
        .await
        .expect("Limited's own 1KB cap, not the app-wide 1-byte one");
    assert!(value.ok);
}
