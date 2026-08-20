//! Unit tests for `actix/middleware/auth.rs`.
//!
//! `GATEHOUSE_URL` is process-global and also touched by realm/jwks/sso_client
//! tests elsewhere in this binary - every test here that reads or sets it
//! holds `env_lock::ENV_LOCK` for its whole body, including across the
//! `.await` points that follow - each test runs on its own thread here, so
//! nothing else can deadlock on it.
#![allow(clippy::await_holding_lock)]

use actix_web::dev::{Service, Transform};
use actix_web::web;
use actix_web::{http::StatusCode, test};
use quench_auth::actix::domain::jwt::JwtConfig;
use quench_auth::actix::domain::session::SessionDb;
use quench_auth::actix::middleware::auth::Auth;
use quench_cache::CacheStore;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "sage".to_string();
    config.audiences = vec!["sage".to_string()];
    config.auth_enabled = true;
    config
}

async fn middleware(
    config: JwtConfig,
) -> impl Service<
    actix_web::dev::ServiceRequest,
    Response = actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>,
    Error = actix_web::Error,
> {
    Auth::new(config)
        .new_transform(test::ok_service())
        .await
        .expect("transform never fails")
}

fn spawn_server(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

#[actix_web::test]
async fn auth_disabled_bypasses_verification_entirely() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default().uri("/x").to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[actix_web::test]
async fn no_token_at_all_is_unauthorized_with_a_www_authenticate_challenge() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::default().uri("/x").to_srv_request();
    let res = mw.call(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.response().headers().get("WWW-Authenticate").unwrap(),
        "Bearer"
    );
}

#[actix_web::test]
async fn a_browser_with_no_token_is_redirected_to_the_gatehouse_login_when_configured() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");

    let mw = middleware(config()).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Accept", "text/html"))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res
        .response()
        .headers()
        .get("Location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("https://gate.example.com/ui/login"));
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn a_browser_with_no_token_falls_back_to_401_when_gatehouse_is_not_configured() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("GATEHOUSE_URL");

    let mw = middleware(config()).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Accept", "text/html"))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_garbage_bearer_token_is_unauthorized() {
    let mw = middleware(config()).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", "Bearer not-a-real-token"))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_valid_bearer_token_for_this_service_is_accepted() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "user".to_string(), None)
        .await
        .unwrap();
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[actix_web::test]
async fn a_token_valid_for_a_different_service_is_unauthorized() {
    let cfg = config();
    // Issued for "warehouse" only - this middleware is guarding "sage".
    let token = cfg
        .issue_access_token_for(
            "someone".to_string(),
            vec!["warehouse".to_string()],
            "user".to_string(),
            None,
        )
        .await
        .unwrap();
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_session_bound_token_needs_an_active_session_in_app_data() {
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
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .app_data(web::Data::new(session_db.clone()))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[actix_web::test]
async fn a_session_bound_token_is_unauthorized_once_the_session_is_revoked() {
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
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .app_data(web::Data::new(session_db.clone()))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn a_session_bound_token_is_unauthorized_with_no_session_db_mounted_at_all() {
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
    let mw = middleware(cfg).await;

    let req = test::TestRequest::default()
        .uri("/x")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn no_access_token_but_a_valid_refresh_cookie_is_silently_refreshed() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let cfg = config();
    // The mock gatehouse hands back a real, valid access token so the retry
    // through `authenticate` succeeds and the request is let through.
    let fresh_access = cfg
        .issue_access_token("someone".to_string(), "user".to_string(), None)
        .await
        .unwrap();
    let body = serde_json::json!({
        "access_token": fresh_access,
        "refresh_token": "brand-new-refresh-token",
    })
    .to_string();
    let url = spawn_server(body);
    envmnt::set("GATEHOUSE_URL", &url);

    let mw = middleware(cfg).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .cookie(actix_web::cookie::Cookie::new(
            "forge_refresh",
            "old-refresh-token",
        ))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let cookies: Vec<String> = res
        .response()
        .headers()
        .get_all("set-cookie")
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect();
    assert!(cookies.iter().any(|c| c.contains("forge_session=")));
    assert!(
        cookies
            .iter()
            .any(|c| c.contains("forge_refresh=brand-new-refresh-token"))
    );
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn a_refresh_that_yields_an_unusable_access_token_is_unauthorized() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let body = serde_json::json!({
        "access_token": "not-a-real-jwt",
        "refresh_token": "brand-new-refresh-token",
    })
    .to_string();
    let url = spawn_server(body);
    envmnt::set("GATEHOUSE_URL", &url);

    let mw = middleware(config()).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .cookie(actix_web::cookie::Cookie::new(
            "forge_refresh",
            "old-refresh-token",
        ))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn a_refresh_cookie_with_no_reachable_gatehouse_is_unauthorized() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");

    let mw = middleware(config()).await;
    let req = test::TestRequest::default()
        .uri("/x")
        .cookie(actix_web::cookie::Cookie::new(
            "forge_refresh",
            "old-refresh-token",
        ))
        .to_srv_request();
    let res = mw.call(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    envmnt::remove("GATEHOUSE_URL");
}
