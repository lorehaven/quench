//! Unit tests for `actix/domain/sso_client.rs`.
//!
//! `GATEHOUSE_URL`/`GATEHOUSE_CLIENT_ID`/`GATEHOUSE_CLIENT_SECRET`/
//! `GATEHOUSE_TLS_VERIFY` are process-global and also touched by
//! realm/jwks tests elsewhere in this binary, so every test here holds
//! `env_lock::ENV_LOCK` for its whole body.
//!
//! `AuthorizeState`'s JSON shape (`{"state", "code_verifier", "redirect"}`)
//! is a private implementation detail of the module under test, not a public
//! contract - these tests reach into it only because there is no other way
//! to drive `callback` without a real `authorize_redirect` round trip first.
//!
//! `env_lock::ENV_LOCK` is deliberately held across `.await` points below -
//! each test runs on its own thread here, so nothing else can deadlock on it.
#![allow(clippy::await_holding_lock)]

use actix_web::cookie::Cookie;
use actix_web::test::TestRequest;
use quench_auth::actix::domain::realm::AUTHORIZE_STATE_COOKIE;
use quench_auth::actix::domain::sso_client::{authorize_redirect, callback, refresh};
use std::io::{Read, Write};
use std::net::TcpListener;

fn spawn_server(status_line: &'static str, body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    format!("http://{addr}")
}

fn set_gatehouse(url: &str, client_id: &str, client_secret: &str) {
    envmnt::set("GATEHOUSE_URL", url);
    envmnt::set("GATEHOUSE_CLIENT_ID", client_id);
    envmnt::set("GATEHOUSE_CLIENT_SECRET", client_secret);
    envmnt::remove("GATEHOUSE_TLS_VERIFY");
}

fn clear_gatehouse() {
    envmnt::remove("GATEHOUSE_URL");
    envmnt::remove("GATEHOUSE_CLIENT_ID");
    envmnt::remove("GATEHOUSE_CLIENT_SECRET");
    envmnt::remove("GATEHOUSE_TLS_VERIFY");
}

#[tokio::test]
async fn authorize_redirect_is_unavailable_with_no_gatehouse_url_at_all() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    clear_gatehouse();

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default().to_http_request();
    let res = authorize_redirect(&req, &config);
    assert_eq!(
        res.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn authorize_redirect_is_unavailable_without_client_credentials() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    // Gatehouse URL is set, but the client id/secret this service would use
    // to identify itself are not - `configured()` must still refuse.
    set_gatehouse("https://gate.example.com", "", "");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default().to_http_request();
    let res = authorize_redirect(&req, &config);
    assert_eq!(
        res.status(),
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    );
    clear_gatehouse();
}

#[tokio::test]
async fn authorize_redirect_builds_the_authorize_url_and_sets_the_state_cookie() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    set_gatehouse("https://gate.example.com", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default().to_http_request();
    let res = authorize_redirect(&req, &config);

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    let location = res
        .headers()
        .get("Location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(location.starts_with("https://gate.example.com/api/v1/authorize?"));
    assert!(location.contains("client_id=my-client"));
    assert!(location.contains("state="));
    assert!(location.contains("code_challenge="));
    assert!(location.contains("code_challenge_method=S256"));

    let set_cookie = res
        .headers()
        .get_all("set-cookie")
        .find_map(|value| value.to_str().ok())
        .expect("an authorize-state cookie is set");
    assert!(set_cookie.starts_with(&format!("{AUTHORIZE_STATE_COOKIE}=")));
    clear_gatehouse();
}

fn authorize_state_cookie(state: &str, code_verifier: &str, redirect: &str) -> Cookie<'static> {
    let value = serde_json::json!({
        "state": state,
        "code_verifier": code_verifier,
        "redirect": redirect,
    })
    .to_string();
    Cookie::new(AUTHORIZE_STATE_COOKIE, value)
}

#[tokio::test]
async fn callback_without_the_authorize_cookie_redirects_back_to_login() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    set_gatehouse("https://gate.example.com", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    let location = res.headers().get("Location").unwrap().to_str().unwrap();
    assert!(location.ends_with("/login"));
    clear_gatehouse();
}

#[tokio::test]
async fn callback_with_a_corrupt_authorize_cookie_redirects_back_to_login() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    set_gatehouse("https://gate.example.com", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .cookie(Cookie::new(AUTHORIZE_STATE_COOKIE, "not json"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_missing_the_code_or_state_query_params_redirects_back_to_login() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    set_gatehouse("https://gate.example.com", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback") // no ?code=/&state=
        .cookie(authorize_state_cookie("xyz", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_rejects_a_state_that_does_not_match_the_cookie() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    set_gatehouse("https://gate.example.com", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=wrong")
        .cookie(authorize_state_cookie("expected", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_falls_back_to_login_when_gatehouse_is_unreachable() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    // Nothing listens on this port - the token exchange POST itself fails.
    set_gatehouse("http://127.0.0.1:1", "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .cookie(authorize_state_cookie("xyz", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_falls_back_to_login_when_gatehouse_rejects_the_code_exchange() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let url = spawn_server("HTTP/1.1 400 Bad Request", "{}".to_string());
    set_gatehouse(&url, "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .cookie(authorize_state_cookie("xyz", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_falls_back_to_login_when_gatehouse_returns_an_unreadable_body() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let url = spawn_server("HTTP/1.1 200 OK", "not json".to_string());
    set_gatehouse(&url, "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .cookie(authorize_state_cookie("xyz", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    clear_gatehouse();
}

#[tokio::test]
async fn callback_on_success_redirects_to_the_saved_destination_and_sets_session_cookies() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let body = serde_json::json!({
        "access_token": "the-access-token",
        "refresh_token": "the-refresh-token",
    })
    .to_string();
    let url = spawn_server("HTTP/1.1 200 OK", body);
    set_gatehouse(&url, "my-client", "s3cr3t");

    let config = quench_auth::actix::domain::sso_client::SsoConfig::init();
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .cookie(authorize_state_cookie("xyz", "verifier", "/ui/home"))
        .to_http_request();
    let res = callback(&req, &config).await;

    assert_eq!(res.status(), actix_web::http::StatusCode::FOUND);
    assert_eq!(
        res.headers().get("Location").unwrap().to_str().unwrap(),
        "/ui/home"
    );

    let cookies: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .filter_map(|value| value.to_str().ok().map(str::to_string))
        .collect();
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.contains("forge_session=the-access-token"))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.contains("forge_refresh=the-refresh-token"))
    );
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with(&format!("{AUTHORIZE_STATE_COOKIE}=")))
    );
    clear_gatehouse();
}

#[tokio::test]
async fn refresh_returns_none_with_no_gatehouse_configured() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    clear_gatehouse();
    assert!(refresh("some-refresh-token").await.is_none());
}

#[tokio::test]
async fn refresh_returns_none_when_gatehouse_is_unreachable() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    assert!(refresh("some-refresh-token").await.is_none());
    clear_gatehouse();
}

#[tokio::test]
async fn refresh_returns_none_when_gatehouse_rejects_the_token() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let url = spawn_server("HTTP/1.1 401 Unauthorized", "{}".to_string());
    envmnt::set("GATEHOUSE_URL", &url);
    assert!(refresh("stale-refresh-token").await.is_none());
    clear_gatehouse();
}

#[tokio::test]
async fn refresh_returns_a_fresh_pair_on_success() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let body = serde_json::json!({
        "access_token": "new-access",
        "refresh_token": "new-refresh",
    })
    .to_string();
    let url = spawn_server("HTTP/1.1 200 OK", body);
    envmnt::set("GATEHOUSE_URL", &url);

    let tokens = refresh("old-refresh-token").await.expect("fresh pair");
    assert_eq!(tokens.access_token, "new-access");
    assert_eq!(tokens.refresh_token, "new-refresh");
    clear_gatehouse();
}
