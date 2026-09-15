//! Same scenarios as `tests/unit/actix_middleware_auth_tests.rs`, against
//! the quench-http port - proof the port is behaviorally equivalent, not
//! just that it compiles.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::domain::session::SessionDb;
use quench_auth::http::middleware::auth::Auth;
use quench_cache::CacheStore;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::extract::{Extension, FromRequest};
use quench_http::middleware::wrap;
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::{Arc, Mutex};

// `GATEHOUSE_URL` is process-global; only one test in this binary touches
// it, but the lock is here so a future one that does can't race it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn config() -> JwtConfig {
    let mut config = JwtConfig::for_tests_with_signing();
    config.service_name = "sage".to_string();
    config.audiences = vec!["sage".to_string()];
    config.auth_enabled = true;
    config
}

struct Ok200;

#[async_trait::async_trait]
impl Endpoint for Ok200 {
    async fn call(&self, mut req: Request) -> Response {
        // Proves claims actually landed in extensions, not just that the
        // chain reached here.
        if let Ok(Extension(claims)) =
            Extension::<quench_auth::domain::jwt::Claims>::from_request(&mut req).await
        {
            return Response::text(StatusCode::OK, claims.sub);
        }
        Response::new(StatusCode::OK)
    }
}

async fn app(config: JwtConfig) -> Arc<dyn Endpoint> {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let inner: Arc<dyn Endpoint> = Arc::new(Ok200);
    let _ = container; // container only matters when a session_db test provides one
    wrap(inner, Auth::new(config))
}

async fn app_with_session_db(
    config: JwtConfig,
    session_db: Arc<SessionDb>,
) -> (Arc<dyn Endpoint>, Arc<quench_http::di::Container>) {
    let container = Arc::new(
        ContainerBuilder::new()
            .provide_arc(session_db)
            .build()
            .await
            .unwrap(),
    );
    let inner: Arc<dyn Endpoint> = Arc::new(Ok200);
    (wrap(inner, Auth::new(config)), container)
}

fn request(container: &Arc<quench_http::di::Container>, headers: &[(&str, &str)]) -> Request {
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        map.insert(
            http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            v.parse().unwrap(),
        );
    }
    Request::new(
        Method::GET,
        "/x".parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn auth_disabled_bypasses_verification_entirely() {
    let mut cfg = config();
    cfg.auth_enabled = false;
    let app = app(cfg).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app.call(request(&container, &[])).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn no_token_at_all_is_unauthorized_with_a_www_authenticate_challenge() {
    let app = app(config()).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app.call(request(&container, &[])).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let hyper_resp = resp.into_hyper();
    assert_eq!(
        hyper_resp.headers().get("www-authenticate").unwrap(),
        "Bearer"
    );
}

#[tokio::test]
async fn a_browser_with_no_token_is_redirected_to_the_gatehouse_login_when_configured() {
    // Held only around the env mutation, not the `.await`s below: this is
    // the one test in this binary that touches `GATEHOUSE_URL`, so nothing
    // else can race the read - the lock is future-proofing for whenever a
    // second one is added, not a correctness requirement today.
    {
        let _guard = ENV_LOCK.lock().unwrap();
        envmnt::set("GATEHOUSE_URL", "https://gate.example.com");
    }

    let app = app(config()).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let resp = app
        .call(request(&container, &[("accept", "text/html")]))
        .await;

    assert_eq!(resp.status(), StatusCode::FOUND);
    let hyper_resp = resp.into_hyper();
    let location = hyper_resp
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("https://gate.example.com/ui/login"));

    envmnt::remove("GATEHOUSE_URL");
}

#[tokio::test]
async fn a_garbage_bearer_token_is_unauthorized() {
    let app = app(config()).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let resp = app
        .call(request(
            &container,
            &[("authorization", "Bearer not-a-real-token")],
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_valid_bearer_token_for_this_service_is_accepted_and_claims_reach_the_handler() {
    let cfg = config();
    let token = cfg
        .issue_access_token("someone".to_string(), "user".to_string(), None)
        .await
        .unwrap();
    let app = app(cfg).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app
        .call(request(
            &container,
            &[("authorization", &format!("Bearer {token}"))],
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let hyper_resp = resp.into_hyper();
    let body = http_body_util::BodyExt::collect(hyper_resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(&body[..], b"someone");
}

#[tokio::test]
async fn a_token_valid_for_a_different_service_is_unauthorized() {
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
    let app = app(cfg).await;
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());

    let resp = app
        .call(request(
            &container,
            &[("authorization", &format!("Bearer {token}"))],
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_session_bound_token_needs_an_active_session_in_the_container() {
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
    let (app, container) = app_with_session_db(cfg, session_db.clone()).await;

    let resp = app
        .call(request(
            &container,
            &[("authorization", &format!("Bearer {token}"))],
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    session_db.revoke(&session.id, "someone").await.unwrap();
    let resp = app
        .call(request(
            &container,
            &[("authorization", &format!("Bearer {token}"))],
        ))
        .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
