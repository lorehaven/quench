//! The smaller HTTP pieces: error shapes, middleware, redirects and static assets.

use quench_core::bytes::Bytes;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use quench_http::prelude::{Endpoint, IntoResponse, Middleware, Request, Response};
use quench_starter::http::domain::api_error::{ApiError, json_error};
use quench_starter::http::domain::error;
use quench_starter::http::middleware::correlation::{CorrelationId, inject_correlation_id_header};
use quench_starter::http::middleware::logger::FilteredLogger;
use quench_starter::http::routers::ui;
use std::sync::Arc;

async fn request(uri: &str, headers: &[(&str, &str)]) -> Request {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let mut map = HeaderMap::new();
    for (k, v) in headers {
        map.insert(*k, HeaderValue::from_str(v).unwrap());
    }
    Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(Bytes::new()),
        container,
    )
}

async fn body(response: Response) -> String {
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

struct Status(StatusCode);

#[quench_core::async_trait]
impl Endpoint for Status {
    async fn call(&self, _req: Request) -> Response {
        Response::new(self.0)
    }
}

#[quench_test::test]
async fn error_bodies_have_the_documented_shapes() {
    let plain = json_error(StatusCode::BAD_REQUEST, "nope");
    assert_eq!(plain.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body(plain).await, r#"{"error":"nope"}"#);

    let api = ApiError::new(StatusCode::NOT_FOUND, "missing thing");
    assert_eq!(
        body(api.into_response()).await,
        r#"{"error":"missing thing"}"#
    );
    let server_side = ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, String::from("boom"));
    let response = IntoResponse::into_response(server_side);
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let denied = error::response(StatusCode::FORBIDDEN, error::DENIED, "not allowed");
    assert_eq!(
        body(denied).await,
        r#"{"errors":[{"code":"DENIED","message":"not allowed","detail":{}}]}"#
    );
    let detailed = error::response_with_detail(
        StatusCode::UNAUTHORIZED,
        error::UNAUTHORIZED,
        "who?",
        quench_serde::json!({"realm": "forge"}),
    );
    assert!(
        body(detailed)
            .await
            .contains(r#""detail":{"realm":"forge"}"#)
    );
    assert_eq!(error::UNSUPPORTED, "UNSUPPORTED");
}

#[quench_test::test]
async fn correlation_ids_are_kept_or_minted() {
    let next = Status(StatusCode::OK);
    let kept = CorrelationId
        .handle(
            request("/", &[("x-correlation-id", "abc-123")]).await,
            &next,
        )
        .await;
    assert_eq!(kept.headers().get("x-correlation-id").unwrap(), "abc-123");
    let minted = CorrelationId.handle(request("/", &[]).await, &next).await;
    let id = minted
        .headers()
        .get("x-correlation-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(id.len(), 36, "a UUID: {id}");
    assert!(format!("{CorrelationId:?}").contains("CorrelationId"));

    let client = quench_client::transport::Client::new();
    let builder = inject_correlation_id_header(client.get("http://127.0.0.1:1/"), "abc");
    assert!(format!("{builder:?}").contains("x-correlation-id"));
}

#[quench_test::test]
async fn the_logger_passes_responses_through_untouched() {
    quench_core::env::set("LOG_SKIP_PREFIXES", " /health , ,/assets");
    let logger = FilteredLogger::default();
    quench_core::env::remove("LOG_SKIP_PREFIXES");
    assert!(format!("{logger:?}").contains("/assets"));
    for (path, status) in [
        ("/health", StatusCode::OK),
        ("/health", StatusCode::BAD_GATEWAY),
        ("/other", StatusCode::OK),
        ("/assets/x.css", StatusCode::NOT_FOUND),
    ] {
        let response = logger
            .handle(request(path, &[]).await, &Status(status))
            .await;
        assert_eq!(response.status(), status);
    }
    assert!(
        FilteredLogger::default()
            .clone()
            .handle(request("/", &[]).await, &Status(StatusCode::OK))
            .await
            .status()
            .is_success()
    );
}

#[quench_test::test]
async fn ui_redirects_target_the_login_page() {
    let login = ui::ui_login_redirect();
    assert_eq!(login.status(), StatusCode::FOUND);
    assert!(
        login
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/ui/login")
    );

    let browser = ui::ui_login_redirect_for(&request("/page", &[]).await);
    assert_eq!(browser.status(), StatusCode::FOUND);
    let htmx = ui::ui_login_redirect_for(&request("/fragment", &[("hx-request", "true")]).await);
    assert_eq!(
        htmx.status(),
        StatusCode::OK,
        "htmx ignores the headers of an error response"
    );
    assert!(htmx.headers().contains_key("hx-redirect"));

    assert_eq!(ui::server_root_redirect().await.status(), StatusCode::FOUND);
    assert!(ui::ui_path("/x").ends_with("/ui/x"));
    assert!(ui::ui_asset_path("/app.css").ends_with("/ui/assets/app.css"));
    let base = ui::base_path_redirect().await;
    assert!(
        base.headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("/ui")
    );
    assert_eq!(
        ui::base_path_slash_redirect().await.status(),
        StatusCode::FOUND
    );
}

#[quench_test::test]
async fn static_assets_are_served_safely() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("css")).unwrap();
    for (name, content) in [
        ("css/app.css", "body{}"),
        ("app.js", "1"),
        ("data.json", "{}"),
        ("logo.svg", "<svg/>"),
        ("a.png", "p"),
        ("b.jpg", "j"),
        ("c.jpeg", "j"),
        ("favicon.ico", "i"),
        ("blob.bin", "b"),
    ] {
        std::fs::write(dir.path().join(name), content).unwrap();
    }
    let root = dir.path().to_str().unwrap();
    for (path, content_type) in [
        ("css/app.css", "text/css; charset=utf-8"),
        ("app.js", "application/javascript; charset=utf-8"),
        ("data.json", "application/json; charset=utf-8"),
        ("logo.svg", "image/svg+xml"),
        ("a.png", "image/png"),
        ("b.jpg", "image/jpeg"),
        ("c.jpeg", "image/jpeg"),
        ("favicon.ico", "image/x-icon"),
        ("blob.bin", "application/octet-stream"),
    ] {
        let response = ui::serve_assets(path, root).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            content_type,
            "{path}"
        );
        assert!(
            response
                .headers()
                .get("cache-control")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("max-age")
        );
    }
    assert_eq!(
        ui::serve_assets("missing.css", root).await.status(),
        StatusCode::NOT_FOUND
    );
    for bad in ["", "../secret", "/etc/passwd", "css/../../x", "./app.js"] {
        assert_eq!(
            ui::serve_assets(bad, root).await.status(),
            StatusCode::BAD_REQUEST,
            "{bad:?}"
        );
    }
}
