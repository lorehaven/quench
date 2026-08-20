//! Unit tests for `actix/routers/ui/pages/auth.rs`.
//!
//! `env_lock::ENV_LOCK` is deliberately held across `.await` points below -
//! each test runs on its own thread here, so nothing else can deadlock on it.
#![allow(clippy::await_holding_lock)]

use actix_web::http::StatusCode;
use actix_web::test::TestRequest;
use quench_auth::actix::domain::jwt::JwtConfig;
use quench_auth::actix::domain::sso_client::SsoConfig;
use quench_auth::actix::routers::ui::pages::auth::*;
use std::io::{Read, Write};
use std::net::TcpListener;

/// A login page that redirects anywhere is a phishing primitive, so
/// off-realm targets are dropped rather than followed.
///
/// `AUTH_REDIRECT_HOSTS` is also touched by the other tests below, hence the
/// shared lock.
#[test]
fn only_rooted_paths_and_allowed_hosts_are_followed() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("AUTH_REDIRECT_HOSTS");

    assert_eq!(validated_redirect("/ui/home"), Some("/ui/home".to_string()));
    assert_eq!(validated_redirect("https://evil.example.com/"), None);
    // Protocol-relative URLs look like paths but navigate off-origin.
    assert_eq!(validated_redirect("//evil.example.com"), None);
    assert_eq!(validated_redirect("/\\evil.example.com"), None);

    envmnt::set("AUTH_REDIRECT_HOSTS", "https://sage.example.com");
    assert_eq!(
        validated_redirect("https://sage.example.com/ui/home"),
        Some("https://sage.example.com/ui/home".to_string())
    );
    assert_eq!(validated_redirect("https://evil.example.com/"), None);
    envmnt::remove("AUTH_REDIRECT_HOSTS");
}

#[test]
fn allowed_redirect_hosts_trims_and_splits_the_comma_separated_list() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set(
        "AUTH_REDIRECT_HOSTS",
        " https://a.example.com/ , https://b.example.com,, ",
    );
    assert_eq!(
        allowed_redirect_hosts(),
        vec![
            "https://a.example.com".to_string(),
            "https://b.example.com".to_string(),
        ]
    );
    envmnt::remove("AUTH_REDIRECT_HOSTS");
    assert!(allowed_redirect_hosts().is_empty());
}

#[test]
fn redirect_target_decodes_and_validates_the_redirect_query_param() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("AUTH_REDIRECT_HOSTS");

    let req = TestRequest::default()
        .uri("/ui/login?redirect=%2Fui%2Fhome")
        .to_http_request();
    assert_eq!(redirect_target(&req), Some("/ui/home".to_string()));

    let req = TestRequest::default()
        .uri("/ui/login?redirect=https%3A%2F%2Fevil.example.com")
        .to_http_request();
    assert_eq!(redirect_target(&req), None);

    let req = TestRequest::default().uri("/ui/login").to_http_request();
    assert_eq!(redirect_target(&req), None);
}

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

async fn json_body(res: actix_web::HttpResponse) -> serde_json::Value {
    let bytes = actix_web::body::to_bytes(res.into_body()).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[actix_web::test]
async fn auth_status_with_auth_disabled_reports_the_dev_admin_stand_in() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let req = TestRequest::default().to_http_request();
    let res = auth_status(&req, &cfg).await;

    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "dev");
}

#[actix_web::test]
async fn auth_status_without_a_cookie_is_anonymous() {
    let req = TestRequest::default().to_http_request();
    let res = auth_status(&req, &config()).await;

    assert_eq!(res.status(), StatusCode::OK);
    let body = json_body(res).await;
    assert_eq!(body["authenticated"], false);
    assert_eq!(body["username"], serde_json::Value::Null);
}

#[actix_web::test]
async fn auth_status_with_a_garbage_cookie_is_anonymous() {
    let req = TestRequest::default()
        .cookie(session_cookie("not-a-real-token"))
        .to_http_request();
    let res = auth_status(&req, &config()).await;

    let body = json_body(res).await;
    assert_eq!(body["authenticated"], false);
}

#[actix_web::test]
async fn auth_status_with_a_valid_cookie_reports_the_subject_and_comma_split_roles() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "read,write".to_string(), None)
        .await
        .unwrap();
    let req = TestRequest::default()
        .cookie(session_cookie(&token))
        .to_http_request();
    let res = auth_status(&req, &cfg).await;

    let body = json_body(res).await;
    assert_eq!(body["authenticated"], true);
    assert_eq!(body["username"], "someone");
    assert_eq!(body["roles"], serde_json::json!(["read", "write"]));
}

fn spawn_token_server(body: String) -> String {
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
async fn refresh_delegation_without_a_refresh_cookie_is_unauthorized() {
    let req = TestRequest::default().to_http_request();
    let res = refresh_delegation(&req).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn refresh_delegation_with_an_unreachable_gatehouse_is_unauthorized() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");

    let req = TestRequest::default()
        .cookie(actix_web::cookie::Cookie::new("forge_refresh", "stale"))
        .to_http_request();
    let res = refresh_delegation(&req).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn refresh_delegation_on_success_sets_new_session_cookies() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let body = serde_json::json!({
        "access_token": "new-access",
        "refresh_token": "new-refresh",
    })
    .to_string();
    let url = spawn_token_server(body);
    envmnt::set("GATEHOUSE_URL", &url);

    let req = TestRequest::default()
        .cookie(actix_web::cookie::Cookie::new("forge_refresh", "stale"))
        .to_http_request();
    let res = refresh_delegation(&req).await;

    assert_eq!(res.status(), StatusCode::OK);
    let cookies: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .filter_map(|v| v.to_str().ok().map(str::to_string))
        .collect();
    assert!(
        cookies
            .iter()
            .any(|c| c.contains("forge_session=new-access"))
    );
    assert!(
        cookies
            .iter()
            .any(|c| c.contains("forge_refresh=new-refresh"))
    );
    envmnt::remove("GATEHOUSE_URL");
}

fn sso_config() -> SsoConfig {
    SsoConfig::init()
}

#[actix_web::test]
async fn login_delegation_without_a_refresh_cookie_falls_through_to_authorize_redirect() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    envmnt::set("GATEHOUSE_CLIENT_ID", "my-client");
    envmnt::set("GATEHOUSE_CLIENT_SECRET", "s3cr3t");

    let sso = sso_config();
    let req = TestRequest::default().to_http_request();
    let res = login_delegation(&req, &sso).await;

    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("Location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://gate.example.com/api/v1/authorize?"));

    envmnt::remove("GATEHOUSE_URL");
    envmnt::remove("GATEHOUSE_CLIENT_ID");
    envmnt::remove("GATEHOUSE_CLIENT_SECRET");
}

#[actix_web::test]
async fn login_delegation_with_a_valid_refresh_cookie_skips_login_entirely() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    let body = serde_json::json!({
        "access_token": "new-access",
        "refresh_token": "new-refresh",
    })
    .to_string();
    let url = spawn_token_server(body);
    envmnt::set("GATEHOUSE_URL", &url);
    envmnt::set("GATEHOUSE_CLIENT_ID", "my-client");
    envmnt::set("GATEHOUSE_CLIENT_SECRET", "s3cr3t");

    let sso = sso_config();
    let req = TestRequest::default()
        .cookie(actix_web::cookie::Cookie::new("forge_refresh", "stale"))
        .to_http_request();
    let res = login_delegation(&req, &sso).await;

    assert_eq!(res.status(), StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/home")
    );
    let cookies: Vec<String> = res
        .headers()
        .get_all("set-cookie")
        .filter_map(|v| v.to_str().ok().map(str::to_string))
        .collect();
    assert!(
        cookies
            .iter()
            .any(|c| c.contains("forge_session=new-access"))
    );

    envmnt::remove("GATEHOUSE_URL");
    envmnt::remove("GATEHOUSE_CLIENT_ID");
    envmnt::remove("GATEHOUSE_CLIENT_SECRET");
}

#[actix_web::test]
async fn login_delegation_with_a_refresh_that_fails_falls_through_to_authorize_redirect() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    // Unreachable - `refresh` fails, so this must fall through rather than
    // getting stuck unauthenticated.
    envmnt::set("GATEHOUSE_URL", "http://127.0.0.1:1");
    envmnt::set("GATEHOUSE_CLIENT_ID", "my-client");
    envmnt::set("GATEHOUSE_CLIENT_SECRET", "s3cr3t");

    let sso = sso_config();
    let req = TestRequest::default()
        .cookie(actix_web::cookie::Cookie::new("forge_refresh", "stale"))
        .to_http_request();
    let res = login_delegation(&req, &sso).await;

    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("Location").unwrap().to_str().unwrap();
    assert!(location.starts_with("http://127.0.0.1:1/api/v1/authorize?"));

    envmnt::remove("GATEHOUSE_URL");
    envmnt::remove("GATEHOUSE_CLIENT_ID");
    envmnt::remove("GATEHOUSE_CLIENT_SECRET");
}

#[actix_web::test]
async fn auth_callback_delegates_to_the_sso_client_callback() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    let sso = sso_config();
    // No authorize-state cookie at all - `callback` fails closed to `/login`.
    let req = TestRequest::default()
        .uri("/ui/auth/callback?code=abc&state=xyz")
        .to_http_request();
    let res = auth_callback(&req, &sso).await;

    assert_eq!(res.status(), StatusCode::FOUND);
    assert!(
        res.headers()
            .get("Location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/login")
    );
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn logout_delegation_redirects_to_gatehouse_when_configured() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");

    let req = TestRequest::default().to_http_request();
    let res = logout_delegation(&req);

    assert_eq!(res.status(), StatusCode::FOUND);
    let location = res.headers().get("Location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://gate.example.com/ui/logout?redirect="));
    envmnt::remove("GATEHOUSE_URL");
}

#[actix_web::test]
async fn logout_delegation_is_unavailable_without_gatehouse_configured() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("GATEHOUSE_URL");

    let req = TestRequest::default().to_http_request();
    let res = logout_delegation(&req);
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}
