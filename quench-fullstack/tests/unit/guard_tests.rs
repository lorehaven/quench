//! The checks every call passes before anything runs: method, content type, the custom header,
//! and where the request came from.

use super::functions::*;
use super::support::*;
use quench_fullstack::{Ui, UiApp};
use quench_http::prelude::Endpoint;

const BODY: &[u8] = br#"{"a":1,"b":2}"#;

async fn status_of(headers: &[(impl AsRef<str>, impl AsRef<str>)], body: &[u8]) -> (u16, Out) {
    let jwt = quench_auth::domain::jwt::JwtConfig::for_tests();
    let container = container_with(jwt).await;
    let app = UiApp::new(Ui::new(big()));
    let out = out(app.call(post("/_fn/add", headers, body, &container)).await).await;
    (out.status, out)
}

#[quench_test::test]
async fn a_well_formed_same_origin_call_passes() {
    let (status, out) = status_of(&with(RPC, SAME_ORIGIN), BODY).await;
    assert_eq!(status, 200);
    assert_eq!(out.text(), "3");
    assert_eq!(out.header("cache-control").as_deref(), Some("no-store"));
    let _ = add(1, 1).await; // keeps the fixtures linked
}

#[quench_test::test]
async fn a_call_without_the_custom_header_is_refused() {
    let (status, out) = status_of(&[("content-type", "application/json")], BODY).await;
    assert_eq!(status, 403);
    assert!(out.text().contains("forbidden") && out.text().contains("x-quench-rpc"));
    // The wrong value is as good as none.
    let (status, _) = status_of(
        &[("content-type", "application/json"), ("x-quench-rpc", "0")],
        BODY,
    )
    .await;
    assert_eq!(status, 403);
}

#[quench_test::test]
async fn only_post_is_accepted() {
    let jwt = quench_auth::domain::jwt::JwtConfig::for_tests();
    let container = container_with(jwt).await;
    let app = UiApp::new(Ui::new(big()));
    let mut request = request("/_fn/add", &[]).await;
    drop(container);
    let _ = &mut request;
    let out = out(app.call(request).await).await;
    assert_eq!(out.status, 405);
    assert_eq!(out.header("allow").as_deref(), Some("POST"));
}

#[quench_test::test]
async fn the_body_must_be_json() {
    for content_type in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data; boundary=x",
        "",
    ] {
        let (status, _) = status_of(
            &[("content-type", content_type), ("x-quench-rpc", "1")],
            BODY,
        )
        .await;
        assert_eq!(status, 415, "{content_type:?}");
    }
    let (status, _) = status_of(&[("x-quench-rpc", "1")], BODY).await;
    assert_eq!(status, 415, "no content type");
    for content_type in ["application/json; charset=utf-8", "Application/JSON"] {
        let (status, _) = status_of(
            &with(
                &[("content-type", content_type), ("x-quench-rpc", "1")],
                SAME_ORIGIN,
            ),
            BODY,
        )
        .await;
        assert_eq!(status, 200, "{content_type}");
    }
}

#[quench_test::test]
async fn a_request_from_another_site_is_refused_when_it_relies_on_cookies() {
    for site in ["cross-site", "same-site"] {
        let headers = with(RPC, &[("sec-fetch-site", site), ("host", "app.test")]);
        let (status, out) = status_of(&headers, BODY).await;
        assert_eq!(status, 403, "{site}");
        assert!(out.text().contains("cross-site"));
    }
    for site in ["same-origin", "none"] {
        let headers = with(RPC, &[("sec-fetch-site", site)]);
        assert_eq!(status_of(&headers, BODY).await.0, 200, "{site}");
    }
}

#[quench_test::test]
async fn an_origin_that_is_not_this_host_is_refused() {
    let hostile = with(
        RPC,
        &[("origin", "https://evil.example"), ("host", "app.test")],
    );
    let (status, out) = status_of(&hostile, BODY).await;
    assert_eq!(status, 403);
    assert!(out.text().contains("cross-origin"));
    // Ports count.
    let wrong_port = with(
        RPC,
        &[("origin", "https://app.test:8443"), ("host", "app.test")],
    );
    assert_eq!(status_of(&wrong_port, BODY).await.0, 403);
    let right_port = with(
        RPC,
        &[
            ("origin", "https://app.test:8443"),
            ("host", "app.test:8443"),
        ],
    );
    assert_eq!(status_of(&right_port, BODY).await.0, 200);
    // A lookalike is not the host.
    let lookalike = with(
        RPC,
        &[
            ("origin", "https://app.test.evil.example"),
            ("host", "app.test"),
        ],
    );
    assert_eq!(status_of(&lookalike, BODY).await.0, 403);
    // No Origin header (a non-browser client, or an old browser's same-origin POST) is not refused.
    assert_eq!(
        status_of(&with(RPC, &[("host", "app.test")]), BODY).await.0,
        200
    );
}

#[quench_test::test]
async fn a_bearer_call_is_exempt_from_the_origin_checks() {
    // A token in a header is not something a hostile page can attach, so a native shell calling
    // from its own origin is fine.
    let headers = with(
        RPC,
        &[
            ("authorization", "Bearer abc"),
            ("origin", "https://localhost"),
            ("host", "app.test"),
            ("sec-fetch-site", "cross-site"),
        ],
    );
    assert_eq!(status_of(&headers, BODY).await.0, 200);
    // But it still needs the custom header.
    let no_rpc = [
        ("content-type", "application/json"),
        ("authorization", "Bearer abc"),
    ];
    assert_eq!(status_of(&no_rpc, BODY).await.0, 403);
}

#[quench_test::test]
async fn a_huge_body_is_a_client_error_not_an_outage() {
    let big_body = vec![b' '; (1 << 20) + 10];
    let (status, _) = status_of(&with(RPC, SAME_ORIGIN), &big_body).await;
    assert!((400..500).contains(&status), "{status}");
    let empty = status_of(&with(RPC, SAME_ORIGIN), b"").await.0;
    assert_eq!(empty, 422, "an empty body is invalid arguments");
}
