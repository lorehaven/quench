//! Unit tests for `actix/routers/ui/mod.rs`.

use actix_web::HttpMessage;
use actix_web::test::TestRequest;
use actix_web::web;
use quench_auth::actix::domain::jwt::{Claims, JwtConfig};
use quench_auth::actix::domain::session::SessionDb;
use quench_auth::actix::routers::ui::{get_user_from_req, is_ui_authenticated};
use quench_cache::CacheStore;
use std::sync::Arc;

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "sage".to_string();
    config.audiences = vec!["sage".to_string()];
    config.auth_enabled = true;
    config
}

fn session_cookie(value: &str) -> actix_web::cookie::Cookie<'static> {
    actix_web::cookie::Cookie::new("forge_session", value.to_string())
}

#[actix_web::test]
async fn is_ui_authenticated_is_always_true_with_auth_disabled() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let req = TestRequest::default().to_http_request();
    assert!(is_ui_authenticated(&req, &cfg).await);
}

#[actix_web::test]
async fn is_ui_authenticated_without_a_session_cookie_is_false() {
    let req = TestRequest::default().to_http_request();
    assert!(!is_ui_authenticated(&req, &config()).await);
}

#[actix_web::test]
async fn is_ui_authenticated_with_a_garbage_cookie_is_false() {
    let req = TestRequest::default()
        .cookie(session_cookie("not-a-real-token"))
        .to_http_request();
    assert!(!is_ui_authenticated(&req, &config()).await);
}

#[actix_web::test]
async fn is_ui_authenticated_with_a_token_for_another_service_is_false() {
    let cfg = config();
    let token = cfg
        .issue_access_token_for(
            "someone".to_string(),
            vec!["warehouse".to_string()],
            "user".to_string(),
            None,
        )
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .to_http_request();
    assert!(!is_ui_authenticated(&req, &cfg).await);
}

#[actix_web::test]
async fn is_ui_authenticated_with_a_valid_token_and_no_sid_is_true() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "user".to_string(), None)
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .to_http_request();
    assert!(is_ui_authenticated(&req, &cfg).await);
}

#[actix_web::test]
async fn is_ui_authenticated_with_a_sid_and_no_sessiondb_registered_trusts_the_token() {
    let cfg = config();
    let token = cfg
        .issue_access_token_for(
            "someone".to_string(),
            vec!["sage".to_string()],
            "user".to_string(),
            Some("some-session-id".to_string()),
        )
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .to_http_request();
    assert!(is_ui_authenticated(&req, &cfg).await);
}

#[actix_web::test]
async fn is_ui_authenticated_with_an_active_session_is_true_and_a_revoked_one_is_false() {
    let session_db: Arc<SessionDb> = SessionDb::init(CacheStore::in_memory());
    let (session, _refresh_token) = session_db.create("someone", 900).await.unwrap();

    let cfg = config();
    let token = cfg
        .issue_access_token_for(
            "someone".to_string(),
            vec!["sage".to_string()],
            "user".to_string(),
            Some(session.id.clone()),
        )
        .await
        .unwrap();

    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .app_data(web::Data::new(session_db.clone()))
        .to_http_request();
    assert!(is_ui_authenticated(&req, &cfg).await);

    session_db.revoke(&session.id, "someone").await.unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .app_data(web::Data::new(session_db.clone()))
        .to_http_request();
    assert!(!is_ui_authenticated(&req, &cfg).await);
}

#[actix_web::test]
async fn get_user_from_req_prefers_claims_already_in_the_request_extensions() {
    let claims = Claims::for_audiences(
        "already-authenticated".to_string(),
        vec!["sage".to_string()],
        "user".to_string(),
        None,
        900,
    );
    let req = TestRequest::default().to_http_request();
    req.extensions_mut().insert(claims);

    let found = get_user_from_req(&req, &config()).await.unwrap();
    assert_eq!(found.sub, "already-authenticated");
}

#[actix_web::test]
async fn get_user_from_req_with_auth_disabled_and_no_extensions_stands_in_an_admin() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let req = TestRequest::default().to_http_request();

    let found = get_user_from_req(&req, &cfg).await.unwrap();
    assert_eq!(found.sub, "admin");
    assert!(found.allows("sage"));
}

#[actix_web::test]
async fn get_user_from_req_with_auth_enabled_and_no_cookie_is_none() {
    let req = TestRequest::default().to_http_request();
    assert!(get_user_from_req(&req, &config()).await.is_none());
}

#[actix_web::test]
async fn get_user_from_req_with_a_valid_cookie_returns_its_claims() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "user".to_string(), None)
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .to_http_request();

    let found = get_user_from_req(&req, &cfg).await.unwrap();
    assert_eq!(found.sub, "someone");
}

#[actix_web::test]
async fn get_user_from_req_with_an_inactive_session_is_none() {
    let session_db: Arc<SessionDb> = SessionDb::init(CacheStore::in_memory());
    let (session, _refresh_token) = session_db.create("someone", 900).await.unwrap();
    session_db.revoke(&session.id, "someone").await.unwrap();

    let cfg = config();
    let token = cfg
        .issue_access_token_for(
            "someone".to_string(),
            vec!["sage".to_string()],
            "user".to_string(),
            Some(session.id.clone()),
        )
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .app_data(web::Data::new(session_db.clone()))
        .to_http_request();

    assert!(get_user_from_req(&req, &cfg).await.is_none());
}
