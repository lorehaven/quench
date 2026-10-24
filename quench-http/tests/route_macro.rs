//! End-to-end: `#[get]`-annotated handlers are discovered with no manual
//! registration list, resolve a path parameter and an injected service,
//! and dispatch through the real router.
#![allow(clippy::items_after_statements)] // test-local helper types sit next to the code that uses them
#![allow(clippy::unused_async)]
// `#[injectable]` constructors are async by contract

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::error::HttpError;
use quench_http::extract::{Inject, Path};
use quench_http::prelude::{get, injectable};
use quench_http::request::Request;
use quench_proto::{HeaderMap, Method, StatusCode, Uri};
use quench_serde::Deserialize;
use std::sync::Arc;

struct Greeter {
    prefix: &'static str,
}

#[injectable]
async fn provide_greeter() -> Greeter {
    Greeter { prefix: "hello" }
}

#[derive(Deserialize)]
struct UserPath {
    id: String,
}

#[get("/users/{id}")]
async fn get_user(
    Path(params): Path<UserPath>,
    Inject(greeter): Inject<Greeter>,
) -> Result<String, HttpError> {
    Ok(format!("{}, {}", greeter.prefix, params.id))
}

#[quench_test::test]
async fn discovered_route_extracts_path_param_and_injected_service() {
    let container = Arc::new(
        ContainerBuilder::new()
            .build()
            .await
            .expect("graph resolves"),
    );
    let router = quench_http::route::discover_routes();

    let req = Request::new(
        Method::GET,
        "/users/42".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    );

    let resp = quench_http::endpoint::Endpoint::call(&router, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp
        .into_inner()
        .body
        .collect_limited(1 << 20)
        .await
        .unwrap();
    assert_eq!(&body[..], b"hello, 42");
}

quench_test::main!();
