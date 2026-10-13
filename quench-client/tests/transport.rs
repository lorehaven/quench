//! `quench_client::transport` against a real quench-http server, over plain
//! HTTP and over TLS with a self-signed certificate (`tests/fixtures`).

use async_trait::async_trait;
use http::{HeaderMap, Method, StatusCode};
use quench_client::transport::{Client, Error};
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::{load_tls, serve_http_on, serve_https_on};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

struct App;

#[async_trait]
impl Endpoint for App {
    async fn call(&self, mut req: Request) -> Response {
        let path = req.uri().path().to_string();
        match path.as_str() {
            "/redirect-get" => return redirect(StatusCode::FOUND, "/echo"),
            "/redirect-keep" => return redirect(StatusCode::TEMPORARY_REDIRECT, "/echo"),
            "/loop" => return redirect(StatusCode::FOUND, "/loop"),
            "/slow" => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Response::ok();
            }
            _ => {}
        }
        let body = req
            .take_body()
            .collect_limited(1 << 20)
            .await
            .unwrap_or_default();
        let headers: &HeaderMap = req.headers();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        Response::json(
            StatusCode::OK,
            &json!({
                "method": req.method().as_str(),
                "path": path,
                "authorization": header("authorization"),
                "content_type": header("content-type"),
                "body": String::from_utf8_lossy(&body),
            }),
        )
        .unwrap()
    }
}

fn redirect(status: StatusCode, to: &str) -> Response {
    Response::new(status).header("location", to)
}

async fn plain_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_http_on(
        Arc::new(App),
        container,
        listener,
        std::future::pending(),
    ));
    format!("http://{addr}")
}

async fn tls_server() -> String {
    // The server side (`load_tls`) builds its config from the process default.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let config = load_tls(
        format!("{dir}/localhost.pem"),
        format!("{dir}/localhost.key"),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    tokio::spawn(serve_https_on(
        Arc::new(App),
        container,
        listener,
        config,
        std::future::pending(),
    ));
    format!("https://{addr}")
}

#[tokio::test]
async fn json_post_sends_body_and_content_type() {
    let base = plain_server().await;
    let echoed: Value = Client::new()
        .post(format!("{base}/echo"))
        .json(&json!({"a": 1}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["method"], "POST");
    assert_eq!(echoed["content_type"], "application/json");
    assert_eq!(echoed["body"], r#"{"a":1}"#);
}

#[tokio::test]
async fn form_basic_and_bearer_auth_are_encoded() {
    let base = plain_server().await;
    let client = Client::new();

    let echoed: Value = client
        .post(format!("{base}/echo"))
        .form(&[("grant_type", "authorization_code"), ("code", "a b&c")])
        .basic_auth("Aladdin", Some("open sesame"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["content_type"], "application/x-www-form-urlencoded");
    assert_eq!(echoed["body"], "grant_type=authorization_code&code=a+b%26c");
    assert_eq!(
        echoed["authorization"],
        "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
    );

    let echoed: Value = client
        .get(format!("{base}/echo"))
        .bearer_auth("tok")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["authorization"], "Bearer tok");
}

#[tokio::test]
async fn redirects_are_followed_with_the_right_method() {
    let base = plain_server().await;
    let client = Client::new();

    let got: Value = client
        .post(format!("{base}/redirect-get"))
        .json(&json!({"x": 1}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        (got["method"].as_str(), got["body"].as_str()),
        (Some("GET"), Some(""))
    );

    let kept: Value = client
        .post(format!("{base}/redirect-keep"))
        .json(&json!({"x": 1}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(kept["method"], "POST");
    assert_eq!(kept["body"], r#"{"x":1}"#);
}

#[tokio::test]
async fn a_redirect_loop_gives_up() {
    let base = plain_server().await;
    let err = Client::new()
        .get(format!("{base}/loop"))
        .send()
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Transport(m) if m.contains("redirect")));
}

#[tokio::test]
async fn timeouts_and_connection_failures_are_errors() {
    let base = plain_server().await;
    let client = Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let err = client
        .get(format!("{base}/slow"))
        .send()
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Timeout));

    let err = Client::new()
        .get("http://127.0.0.1:1/")
        .send()
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Transport(_)));
    assert!(matches!(
        Client::new().get("not a url").send().await.err().unwrap(),
        Error::Request(_)
    ));
}

#[tokio::test]
async fn self_signed_tls_is_rejected_unless_verification_is_disabled() {
    let base = tls_server().await;

    let strict = Client::new().get(format!("{base}/echo")).send().await;
    assert!(strict.is_err(), "a self-signed certificate must not verify");

    let lax = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    let echoed: Value = lax
        .get(format!("{base}/echo"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["method"], Method::GET.as_str());
}
