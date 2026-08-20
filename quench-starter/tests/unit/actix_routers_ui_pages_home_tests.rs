//! Unit tests for `actix/routers/ui/pages/home.rs`.

use actix_web::http::StatusCode;
use actix_web::test::TestRequest;
use actix_web::web;
use quench_auth::actix::domain::jwt::JwtConfig;
use quench_starter::actix::routers::ui::pages::home::{handle_home, service_card};

#[actix_web::test]
async fn auth_disabled_renders_the_page_straight_through() {
    let config = web::Data::new(JwtConfig::for_tests());
    let req = TestRequest::default().to_http_request();

    let response = handle_home(req, config, || actix_web::HttpResponse::Ok().body("home")).await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[actix_web::test]
async fn auth_enabled_without_a_session_cookie_redirects_to_login_instead_of_rendering() {
    let mut cfg = JwtConfig::for_tests();
    cfg.auth_enabled = true;
    let config = web::Data::new(cfg);
    let req = TestRequest::default().to_http_request();

    let response = handle_home(req, config, || {
        panic!("render_fn must not run when the visitor isn't authenticated")
    })
    .await;

    assert_eq!(response.status(), StatusCode::FOUND);
    assert!(response.headers().contains_key("location"));
}

#[test]
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
