//! The blocking client against a real server running on a runtime of its own.

use quench_client::blocking::Client;
use quench_core::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::StatusCode;
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::serve_http_on;
use quench_serde::{Value, json};
use std::sync::Arc;
use tokio::net::TcpListener;

struct Echo;

#[async_trait]
impl Endpoint for Echo {
    async fn call(&self, mut req: Request) -> Response {
        match req.uri().path() {
            "/redirect" => return Response::new(StatusCode::FOUND).header("location", "/elsewhere"),
            "/cookie" => return Response::ok().append_header("set-cookie", "k=v; Path=/"),
            _ => {}
        }
        let body = req
            .take_body()
            .collect_limited(1 << 20)
            .await
            .unwrap_or_default();
        let auth = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        Response::json(
            StatusCode::OK,
            &json!({
                "method": req.method().as_str(),
                "uri": req.uri().to_string(),
                "authorization": auth,
                "body": String::from_utf8_lossy(&body),
            }),
        )
        .unwrap()
    }
}

fn server(runtime: &tokio::runtime::Runtime) -> String {
    runtime.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        tokio::spawn(serve_http_on(
            Arc::new(Echo),
            container,
            listener,
            std::future::pending(),
        ));
        format!("http://{addr}")
    })
}

#[test]
fn blocking_calls_return_when_the_response_is_in() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let base = server(&runtime);
    let client = Client::builder()
        .user_agent("t/1")
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let got: Value = client
        .get(format!("{base}/a"))
        .query(&[("q", "x y")])
        .bearer_auth("tok")
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(
        (got["method"].as_str(), got["authorization"].as_str()),
        (Some("GET"), Some("Bearer tok"))
    );
    assert_eq!(got["uri"], "/a?q=x%20y");

    let posted = client
        .post(format!("{base}/b"))
        .json(&json!({"k": 1}))
        .send()
        .unwrap();
    assert_eq!(posted.status(), StatusCode::OK);
    let posted: Value = posted.json().unwrap();
    assert_eq!(posted["body"], r#"{"k":1}"#);

    let text = client
        .put(format!("{base}/c"))
        .body("raw")
        .send()
        .unwrap()
        .text()
        .unwrap();
    assert!(text.contains("\"PUT\"") && text.contains("raw"), "{text}");
    let bytes = client
        .patch(format!("{base}/d"))
        .form(&[("a", "1")])
        .send()
        .unwrap()
        .bytes()
        .unwrap();
    assert!(!bytes.is_empty());
    assert!(
        client
            .head(format!("{base}/e"))
            .send()
            .unwrap()
            .status()
            .is_success()
    );
    assert!(
        client
            .delete(format!("{base}/f"))
            .basic_auth("u", Some("p"))
            .header("x-a", "b")
            .send()
            .unwrap()
            .headers()
            .contains_key("content-type")
    );
    assert!(Client::new().get("http://127.0.0.1:1/").send().is_err());
    drop(runtime);
}

#[test]
fn blocking_redirects_cookies_and_multipart() {
    use quench_client::blocking::Client;
    use quench_client::multipart::Form;
    use quench_client::transport::Redirect;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let base = server(&runtime);

    let client = Client::builder().redirect(Redirect::None).build().unwrap();
    let redirected = client.get(format!("{base}/redirect")).send().unwrap();
    assert_eq!(redirected.status(), StatusCode::FOUND);

    let cookie = client.get(format!("{base}/cookie")).send().unwrap();
    assert_eq!(cookie.cookies(), vec![("k".to_string(), "v".to_string())]);
    assert_eq!(cookie.cookie("k").as_deref(), Some("v"));

    let echoed: Value = client
        .post(format!("{base}/echo"))
        .multipart(Form::new().text("a", "b"))
        .cookies(&[("k", "v")])
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert!(echoed["body"].as_str().unwrap().contains("name=\"a\"\r\n\r\nb"));
}
