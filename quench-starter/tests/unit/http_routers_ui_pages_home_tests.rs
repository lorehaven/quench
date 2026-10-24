//! Unit tests for `http/routers/ui/pages/home.rs`.

use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::{Request, Response};
use quench_starter::http::routers::ui::pages::home::{handle_home, service_card};
use std::sync::Arc;

async fn request() -> Request {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    Request::new(
        Method::GET,
        "/ui".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container,
    )
}

#[quench_test::test]
async fn auth_disabled_renders_the_page_straight_through() {
    let config = JwtConfig::for_tests();
    let req = request().await;

    let response = handle_home(&req, &config, || Response::new(StatusCode::OK)).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[quench_test::test]
async fn auth_enabled_without_a_session_cookie_redirects_to_login_instead_of_rendering() {
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = true;
    let req = request().await;

    let response = handle_home(&req, &config, || {
        panic!("render_fn must not run when the visitor isn't authenticated")
    })
    .await;

    assert_eq!(response.status(), StatusCode::FOUND);
    assert!(response.into_inner().headers.contains_key("location"));
}

#[quench_test::test]
fn service_card_builds_a_link_with_the_extra_class_and_both_i18n_keys() {
    let html = service_card("/ui/jobs", "jobs_title", "jobs_desc", "jobs-card").render();
    assert!(html.starts_with("<a"));
    assert!(html.contains("href=\"/ui/jobs\""));
    assert!(html.contains("class=\"home-card jobs-card\""));
    assert!(html.contains("data-i18n=\"jobs_title\""));
    assert!(html.contains("data-i18n=\"jobs_desc\""));
    assert!(html.contains("home-card-arrow"));
    assert!(html.contains('\u{2192}'));
}
