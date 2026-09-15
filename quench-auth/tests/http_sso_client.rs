//! `crate::http::domain::sso_client` had no tests at all before this -
//! the most complex untested piece of the quench-http port. Covers the
//! validation paths directly (no network needed) and the full successful
//! code-exchange path against a tiny mock gatehouse.
//!
//! Each test holds `ENV_LOCK` across its `.await`s, same as
//! `tests/unit/actix_middleware_auth_tests.rs`: every test here runs on
//! its own thread (`cargo test`'s default), so nothing else on the same
//! runtime can deadlock on it.
#![allow(clippy::await_holding_lock)]

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::sso_client::SsoConfig;
use quench_auth::http::domain::sso_client::{authorize_redirect, callback};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::request::Request;
use std::io::{Read, Write};
use std::net::TcpListener as StdTcpListener;
use std::sync::{Arc, Mutex};

// `GATEHOUSE_URL`/`GATEHOUSE_CLIENT_ID`/`GATEHOUSE_CLIENT_SECRET` are
// process-global; every test in this binary that touches them holds this
// for its whole body.
static ENV_LOCK: Mutex<()> = Mutex::new(());

async fn request(uri: &str, headers: &[(&str, &str)]) -> Request {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        map.insert(
            http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            v.parse().unwrap(),
        );
    }
    Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(Bytes::new()),
        container,
    )
}

fn sso_config() -> SsoConfig {
    envmnt::set("GATEHOUSE_CLIENT_ID", "test-client");
    envmnt::set("GATEHOUSE_CLIENT_SECRET", "test-secret");
    SsoConfig::init()
}

/// A one-shot mock gatehouse: answers exactly one HTTP request with the
/// given JSON body, then stops.
fn spawn_mock_gatehouse(body: &'static str) -> String {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
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

#[tokio::test]
async fn authorize_redirect_without_gatehouse_url_is_service_unavailable() {
    let _guard = ENV_LOCK.lock().unwrap();
    envmnt::remove("GATEHOUSE_URL");
    let config = sso_config();

    let req = request("/ui/login", &[]).await;
    let resp = authorize_redirect(&req, &config);
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn authorize_redirect_builds_a_pkce_authorize_url_and_sets_the_state_cookie() {
    let _guard = ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    let config = sso_config();

    let req = request("/ui/login", &[("host", "myservice.example.com")]).await;
    let resp = authorize_redirect(&req, &config);
    assert_eq!(resp.status(), StatusCode::FOUND);

    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.starts_with("https://gate.example.com/api/v1/authorize?"),
        "{location}"
    );
    assert!(location.contains("client_id=test-client"), "{location}");
    assert!(location.contains("code_challenge="), "{location}");
    assert!(
        location.contains("code_challenge_method=S256"),
        "{location}"
    );

    let set_cookie = hyper_resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        set_cookie.starts_with("forge_authorize_state="),
        "{set_cookie}"
    );

    envmnt::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn callback_without_an_authorize_state_cookie_redirects_to_login() {
    let _guard = ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    let config = sso_config();

    let req = request("/ui/auth/callback?code=abc&state=xyz", &[]).await;
    let resp = callback(&req, &config).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.ends_with("/ui/login"), "{location}");

    envmnt::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn callback_with_a_state_mismatch_redirects_to_login() {
    let _guard = ENV_LOCK.lock().unwrap();
    envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    let config = sso_config();

    let saved_state = r#"{"state":"expected","code_verifier":"verifier","redirect":"/ui/home"}"#;
    let cookie = format!("forge_authorize_state={saved_state}");
    let req = request(
        "/ui/auth/callback?code=abc&state=WRONG",
        &[("cookie", &cookie)],
    )
    .await;

    let resp = callback(&req, &config).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.ends_with("/ui/login"), "{location}");

    envmnt::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn callback_exchanges_the_code_and_sets_session_cookies_on_success() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mock_body = r#"{"access_token":"access-123","refresh_token":"refresh-456"}"#;
    let gatehouse_url = spawn_mock_gatehouse(mock_body);
    envmnt::set("GATEHOUSE_URL", &gatehouse_url);
    let config = sso_config();

    let saved_state = r#"{"state":"expected","code_verifier":"verifier","redirect":"/ui/home"}"#;
    let cookie = format!("forge_authorize_state={saved_state}");
    let req = request(
        "/ui/auth/callback?code=abc&state=expected",
        &[("cookie", &cookie)],
    )
    .await;

    let resp = callback(&req, &config).await;
    assert_eq!(resp.status(), StatusCode::FOUND);

    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(location, "/ui/home");

    let cookies: Vec<&str> = hyper_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("forge_session=access-123")),
        "{cookies:?}"
    );
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("forge_refresh=refresh-456")),
        "{cookies:?}"
    );
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("forge_authorize_state=;")
                || c.starts_with("forge_authorize_state=\"\";")),
        "expected the authorize-state cookie to be cleared: {cookies:?}"
    );

    envmnt::remove("GATEHOUSE_URL");
}
