//! `Extension<T>` (middleware -> handler data, e.g. decoded auth claims)
//! and `Response::append_header` (repeated `Set-Cookie`-shaped headers)
//! exist specifically to port `quench-auth`'s actix middleware, which
//! stashes claims via `HttpMessage::extensions()` and sets two cookies on
//! one response.

use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use http_body_util::BodyExt;
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::extract::{Extension, FromRequest};
use quench_http::middleware::{Middleware, wrap};
use quench_http::request::Request;
use quench_http::response::Response;
use std::sync::Arc;

#[derive(Clone)]
struct Claims {
    sub: String,
}

struct StampClaims;

#[async_trait]
impl Middleware for StampClaims {
    async fn handle(&self, mut req: Request, next: &dyn Endpoint) -> Response {
        req.extensions_mut().insert(Claims {
            sub: "alice".to_string(),
        });
        next.call(req).await
    }
}

struct ReadClaims;

#[async_trait]
impl Endpoint for ReadClaims {
    async fn call(&self, mut req: Request) -> Response {
        match Extension::<Claims>::from_request(&mut req).await {
            Ok(Extension(claims)) => Response::text(StatusCode::OK, claims.sub),
            Err(e) => e.into_response(),
        }
    }
}

fn get(container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        "/".parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn extension_set_by_middleware_is_readable_by_the_handler() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app: Arc<dyn Endpoint> = Arc::new(ReadClaims);
    let app = wrap(app, StampClaims);

    let resp = app.call(get(&container)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp
        .into_hyper()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(&body[..], b"alice");
}

#[tokio::test]
async fn extension_missing_is_a_clear_error_not_a_panic() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let app = ReadClaims;

    let resp = app.call(get(&container)).await;
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn append_header_keeps_both_values_where_a_plain_header_would_overwrite() {
    let resp = Response::ok()
        .append_header("set-cookie", "session=a")
        .append_header("set-cookie", "refresh=b");

    let hyper_resp = resp.into_hyper();
    let values: Vec<&str> = hyper_resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values, vec!["session=a", "refresh=b"]);
}
