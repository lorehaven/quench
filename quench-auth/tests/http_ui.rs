//! The UI-side helpers: who the cookie says you are, the status watcher,
//! refresh and login delegation, logout, and redirect validation.
//!
//! The gatehouse URL and cookie names are process-global, so every test holds `ENV_LOCK`.
#![allow(clippy::await_holding_lock)]

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::realm;
use quench_auth::domain::session::SessionDb;
use quench_auth::domain::sso_client::SsoConfig;
use quench_auth::http::routers::ui::pages::auth::{
    allowed_redirect_hosts, auth_callback, auth_status, login_delegation, logout_delegation,
    redirect_target, refresh_delegation, validated_redirect,
};
use quench_auth::http::routers::ui::{get_user_from_req, is_ui_authenticated};
use quench_cache::CacheStore;
use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::http::{HeaderMap, HeaderName, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_http::response::Response;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn config(auth_enabled: bool) -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "svc".to_string();
    config.audiences = vec!["svc".to_string()];
    config.auth_enabled = auth_enabled;
    config
}

async fn container() -> Arc<Container> {
    Arc::new(ContainerBuilder::new().build().await.unwrap())
}

fn request_in(container: &Arc<Container>, uri: &str, headers: &[(&str, &str)]) -> Request {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

async fn request(uri: &str, headers: &[(&str, &str)]) -> Request {
    request_in(&container().await, uri, headers)
}

fn cookie_header(token: &str) -> String {
    format!("{}={token}", realm::session_cookie_name())
}

async fn body_of(response: Response) -> String {
    String::from_utf8(
        response
            .into_inner()
            .body
            .collect_limited(1 << 20)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

fn location(response: Response) -> String {
    response
        .into_inner()
        .headers
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

/// A mock gatehouse answering every request with `status` and `body` until the test ends.
fn mock_gatehouse(status: &'static str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn authentication_follows_the_session_cookie() {
    let _guard = ENV_LOCK.lock().unwrap();
    let config = config(true);
    let token = config
        .issue_access_token("alice".into(), "user".into(), None)
        .await
        .unwrap();

    assert!(
        is_ui_authenticated(
            &request("/", &[("cookie", &cookie_header(&token))]).await,
            &config
        )
        .await
    );
    assert!(
        !is_ui_authenticated(&request("/", &[]).await, &config).await,
        "no cookie"
    );
    assert!(
        !is_ui_authenticated(
            &request("/", &[("cookie", &cookie_header("garbage"))]).await,
            &config
        )
        .await,
        "bad token"
    );
    assert!(
        is_ui_authenticated(&request("/", &[]).await, &self::config(false)).await,
        "auth disabled"
    );

    let other = config
        .issue_access_token_for(
            "alice".into(),
            vec!["elsewhere".into()],
            "user".into(),
            None,
        )
        .await
        .unwrap();
    assert!(
        !is_ui_authenticated(
            &request("/", &[("cookie", &cookie_header(&other))]).await,
            &config
        )
        .await,
        "token for another service"
    );
}

#[tokio::test]
async fn revoked_sessions_stop_authenticating_everywhere() {
    let _guard = ENV_LOCK.lock().unwrap();
    let config = config(true);
    let sessions = SessionDb::init(CacheStore::in_memory());
    let (session, _) = sessions.create("alice", 3600).await.unwrap();
    let container = Arc::new(
        ContainerBuilder::new()
            .provide_arc(sessions.clone())
            .build()
            .await
            .unwrap(),
    );
    let token = config
        .issue_access_token("alice".into(), "user".into(), Some(session.id.clone()))
        .await
        .unwrap();
    let headers = [("cookie", cookie_header(&token))];
    let req = |c: &Arc<Container>| request_in(c, "/", &[("cookie", headers[0].1.as_str())]);

    assert!(is_ui_authenticated(&req(&container), &config).await);
    assert!(get_user_from_req(&req(&container), &config).await.is_some());
    sessions.revoke(&session.id, "alice").await.unwrap();
    assert!(!is_ui_authenticated(&req(&container), &config).await);
    assert!(get_user_from_req(&req(&container), &config).await.is_none());

    // Without a SessionDb registered the token alone decides.
    assert!(is_ui_authenticated(&req(&self::container().await), &config).await);
}

#[tokio::test]
async fn the_current_user_comes_from_extensions_dev_mode_or_the_cookie() {
    let _guard = ENV_LOCK.lock().unwrap();
    let config = config(true);

    let mut req = request("/", &[]).await;
    req.extensions_mut().insert(Claims::for_audiences(
        "injected".into(),
        vec!["svc".into()],
        "user".into(),
        None,
        60,
    ));
    assert_eq!(
        get_user_from_req(&req, &config).await.unwrap().sub,
        "injected"
    );

    let dev = get_user_from_req(&request("/", &[]).await, &self::config(false))
        .await
        .unwrap();
    assert_eq!((dev.sub.as_str(), dev.scope.as_str()), ("admin", "admin"));

    assert!(
        get_user_from_req(&request("/", &[]).await, &config)
            .await
            .is_none()
    );
    assert!(
        get_user_from_req(
            &request("/", &[("cookie", &cookie_header("junk"))]).await,
            &config
        )
        .await
        .is_none()
    );
    let token = config
        .issue_access_token("bob".into(), "user,admin".into(), None)
        .await
        .unwrap();
    assert_eq!(
        get_user_from_req(
            &request("/", &[("cookie", &cookie_header(&token))]).await,
            &config
        )
        .await
        .unwrap()
        .sub,
        "bob"
    );
    let foreign = config
        .issue_access_token_for("bob".into(), vec!["other".into()], "user".into(), None)
        .await
        .unwrap();
    assert!(
        get_user_from_req(
            &request("/", &[("cookie", &cookie_header(&foreign))]).await,
            &config
        )
        .await
        .is_none()
    );
}

#[tokio::test]
async fn the_status_endpoint_always_answers_200() {
    let _guard = ENV_LOCK.lock().unwrap();
    let config = config(true);
    let anonymous = auth_status(&request("/", &[]).await, &config).await;
    assert_eq!(anonymous.status(), StatusCode::OK);
    assert!(body_of(anonymous).await.contains("\"authenticated\":false"));

    let token = config
        .issue_access_token("alice".into(), "user,admin,".into(), None)
        .await
        .unwrap();
    let known = body_of(
        auth_status(
            &request("/", &[("cookie", &cookie_header(&token))]).await,
            &config,
        )
        .await,
    )
    .await;
    assert!(
        known.contains("\"authenticated\":true")
            && known.contains("alice")
            && known.contains("\"roles\":[\"user\",\"admin\"]"),
        "{known}"
    );

    let junk = body_of(
        auth_status(
            &request("/", &[("cookie", &cookie_header("junk"))]).await,
            &config,
        )
        .await,
    )
    .await;
    assert!(junk.contains("\"authenticated\":false"));
    let dev = body_of(auth_status(&request("/", &[]).await, &self::config(false)).await).await;
    assert!(dev.contains("\"username\":\"dev\""));
}

#[tokio::test]
async fn refresh_delegation_exchanges_the_refresh_cookie() {
    let _guard = ENV_LOCK.lock().unwrap();
    let refresh_cookie = format!("{}=old-refresh", realm::refresh_cookie_name());

    quench_core::env::remove("GATEHOUSE_URL");
    assert_eq!(
        refresh_delegation(&request("/", &[]).await).await.status(),
        StatusCode::UNAUTHORIZED,
        "no cookie"
    );
    assert_eq!(
        refresh_delegation(&request("/", &[("cookie", &refresh_cookie)]).await)
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "no gatehouse"
    );

    quench_core::env::set("GATEHOUSE_URL", mock_gatehouse("401 Unauthorized", "{}"));
    assert_eq!(
        refresh_delegation(&request("/", &[("cookie", &refresh_cookie)]).await)
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "refused"
    );
    quench_core::env::set("GATEHOUSE_URL", mock_gatehouse("200 OK", "not json"));
    assert_eq!(
        refresh_delegation(&request("/", &[("cookie", &refresh_cookie)]).await)
            .await
            .status(),
        StatusCode::UNAUTHORIZED,
        "unreadable answer"
    );

    quench_core::env::set(
        "GATEHOUSE_URL",
        mock_gatehouse(
            "200 OK",
            r#"{"access_token":"new-access","refresh_token":"new-refresh"}"#,
        ),
    );
    let ok = refresh_delegation(&request("/", &[("cookie", &refresh_cookie)]).await).await;
    assert_eq!(ok.status(), StatusCode::OK);
    let inner = ok.into_inner();
    let cookies: Vec<&str> = inner
        .headers
        .get_all("set-cookie")
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(cookies.len(), 2);
    assert!(
        cookies[0].starts_with("forge_session=new-access")
            && cookies[1].starts_with("forge_refresh=new-refresh"),
        "{cookies:?}"
    );
    quench_core::env::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn login_delegation_prefers_a_valid_refresh_cookie() {
    let _guard = ENV_LOCK.lock().unwrap();
    quench_core::env::set("GATEHOUSE_CLIENT_ID", "svc-client");
    quench_core::env::set("GATEHOUSE_CLIENT_SECRET", "svc-secret");
    let sso = SsoConfig::init();
    let refresh_cookie = format!("{}=old", realm::refresh_cookie_name());

    quench_core::env::set(
        "GATEHOUSE_URL",
        mock_gatehouse("200 OK", r#"{"access_token":"a","refresh_token":"r"}"#),
    );
    let skipped = login_delegation(
        &request("/ui/login", &[("cookie", &refresh_cookie)]).await,
        &sso,
    )
    .await;
    assert_eq!(skipped.status(), StatusCode::FOUND);
    let inner = skipped.into_inner();
    assert!(
        inner
            .headers
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/home")
    );
    assert_eq!(inner.headers.get_all("set-cookie").count(), 2);

    quench_core::env::set("GATEHOUSE_URL", "https://gate.example.com");
    let redirected = login_delegation(
        &request("/ui/login", &[("host", "svc.example.com")]).await,
        &sso,
    )
    .await;
    assert_eq!(redirected.status(), StatusCode::FOUND);
    assert!(location(redirected).starts_with("https://gate.example.com/api/v1/authorize?"));
    quench_core::env::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn the_callback_without_a_state_cookie_goes_back_to_login() {
    let _guard = ENV_LOCK.lock().unwrap();
    quench_core::env::set("GATEHOUSE_CLIENT_ID", "svc-client");
    quench_core::env::set("GATEHOUSE_CLIENT_SECRET", "svc-secret");
    quench_core::env::set("GATEHOUSE_URL", "https://gate.example.com");
    let response = auth_callback(
        &request("/ui/auth/callback?code=c&state=s", &[]).await,
        &SsoConfig::init(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FOUND);
    quench_core::env::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn logout_hands_the_browser_to_gatehouse() {
    let _guard = ENV_LOCK.lock().unwrap();
    quench_core::env::remove("GATEHOUSE_URL");
    assert_eq!(
        logout_delegation(&request("/ui/logout", &[]).await).status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    quench_core::env::set("GATEHOUSE_URL", "https://gate.example.com");
    let response = logout_delegation(
        &request(
            "/ui/logout",
            &[
                ("host", "svc.example.com"),
                ("x-forwarded-proto", "http"),
                ("x-forwarded-host", "public.example.com"),
            ],
        )
        .await,
    );
    assert_eq!(response.status(), StatusCode::FOUND);
    let target = location(response);
    assert!(target.starts_with("https://gate.example.com/"), "{target}");
    assert!(target.contains("public.example.com"), "{target}");
    quench_core::env::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn redirect_targets_are_validated() {
    let _guard = ENV_LOCK.lock().unwrap();
    quench_core::env::remove("AUTH_REDIRECT_HOSTS");
    assert_eq!(
        validated_redirect("/home?x=1").as_deref(),
        Some("/home?x=1")
    );
    for bad in [
        "//evil.example",
        "/\\evil",
        "https://evil.example/",
        "javascript:alert(1)",
        "",
    ] {
        assert_eq!(validated_redirect(bad), None, "{bad:?}");
    }
    assert!(allowed_redirect_hosts().is_empty());

    quench_core::env::set(
        "AUTH_REDIRECT_HOSTS",
        " https://app.example.com/ , ,https://other.example.com",
    );
    assert_eq!(
        allowed_redirect_hosts(),
        ["https://app.example.com", "https://other.example.com"]
    );
    assert!(validated_redirect("https://app.example.com/page").is_some());
    assert!(
        validated_redirect("https://app.example.com").is_some()
            && validated_redirect("https://app.example.com?x=1").is_some()
    );
    assert!(
        validated_redirect("https://app.example.com.evil.net/").is_none(),
        "the prefix must end at a boundary"
    );
    assert!(validated_redirect("https://app.example.comevil/").is_none());
    assert!(validated_redirect("https://evil.example/").is_none());
    quench_core::env::remove("AUTH_REDIRECT_HOSTS");

    assert_eq!(
        redirect_target(&request("/login?redirect=%2Fdashboard%3Ftab%3D2", &[]).await).as_deref(),
        Some("/dashboard?tab=2")
    );
    assert_eq!(
        redirect_target(&request("/login?other=1&redirect=%2F%2Fevil.example", &[]).await),
        None
    );
    assert_eq!(redirect_target(&request("/login", &[]).await), None);
    assert_eq!(
        redirect_target(&request("/login?redirect=%zz", &[]).await),
        None
    );
}
