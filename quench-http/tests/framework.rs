//! The framework's building blocks one by one: bodies, extractors, responses,
//! routing and the server entry points.

use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_core::stream::{self, StreamExt};
use quench_http::body::InboundBody;
use quench_http::di::{Container, ContainerBuilder};
use quench_http::endpoint::{Endpoint, IntoResponse};
use quench_http::error::{ExtractError, HttpError};
use quench_http::extract::{
    BodyLimit, Extension, Form, FromRequest, Inject, Json, Limited, Path, Query,
};
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::router::{Router, mount};
use quench_proto::{Body, HeaderMap, HeaderValue, Method, StatusCode, Uri};
use quench_serde::{Deserialize, Serialize, json};
use std::future::Future;
use std::sync::Arc;

async fn container() -> Arc<Container> {
    Arc::new(ContainerBuilder::new().build().await.unwrap())
}

fn request(
    method: Method,
    uri: &str,
    headers: HeaderMap,
    body: impl Into<Bytes>,
    container: &Arc<Container>,
) -> Request {
    Request::new(
        method,
        uri.parse::<Uri>().unwrap(),
        headers,
        InboundBody::from_bytes(body),
        container.clone(),
    )
}

async fn simple(uri: &str, body: &'static str) -> Request {
    request(
        Method::POST,
        uri,
        HeaderMap::new(),
        Bytes::from_static(body.as_bytes()),
        &container().await,
    )
}

struct Probe<F>(F);

#[async_trait]
impl<F, Fut> Endpoint for Probe<F>
where
    F: Fn(Request) -> Fut + Send + Sync,
    Fut: Future<Output = Response> + Send,
{
    async fn call(&self, req: Request) -> Response {
        (self.0)(req).await
    }
}

async fn body_text(response: Response) -> String {
    let bytes = response
        .into_inner()
        .body
        .collect_limited(1 << 20)
        .await
        .unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---- bodies ---------------------------------------------------------------------

#[tokio::test]
async fn inbound_bodies_collect_within_their_limit() {
    assert_eq!(InboundBody::empty().collect_limited(10).await.unwrap(), "");
    assert_eq!(
        InboundBody::from_bytes("hello")
            .collect_limited(5)
            .await
            .unwrap(),
        "hello"
    );
    assert!(matches!(
        InboundBody::from_bytes("hello").collect_limited(4).await,
        Err(ExtractError::BodyTooLarge(4))
    ));
    let failing = Body::from_stream(stream::iter(vec![Err::<Bytes, _>(std::io::Error::other(
        "cut off",
    ))]));
    let err = InboundBody::from(failing)
        .collect_limited(10)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ExtractError::BodyReadFailed(m) if m.contains("cut off")),
        "{err}"
    );
}

#[tokio::test]
async fn inbound_bodies_stream_and_convert() {
    let mut body = InboundBody::from_bytes("abc");
    assert_eq!(body.next().await.unwrap().unwrap(), "abc");
    assert!(body.next().await.is_none());
    assert_eq!(
        InboundBody::from_bytes("xyz").into_body().exact_len(),
        Some(3)
    );
    assert!(format!("{:?}", InboundBody::empty()).contains("Body"));
}

// ---- IntoResponse ---------------------------------------------------------------

#[derive(Serialize)]
struct Item {
    id: u32,
}

#[tokio::test]
async fn handler_results_become_responses() {
    assert_eq!(().into_response().status(), StatusCode::NO_CONTENT);
    let text = "plain".into_response();
    assert_eq!(
        (text.status(), body_text(text).await),
        (StatusCode::OK, "plain".to_string())
    );
    let owned = String::from("owned").into_response();
    assert_eq!(body_text(owned).await, "owned");
    let json_response = Json(Item { id: 7 }).into_response();
    assert_eq!(
        json_response.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert_eq!(body_text(json_response).await, r#"{"id":7}"#);
    let err = HttpError::status(StatusCode::IM_A_TEAPOT, "short and stout").into_response();
    assert_eq!(
        (err.status(), body_text(err).await),
        (StatusCode::IM_A_TEAPOT, "short and stout".to_string())
    );
    let ok: Result<String, HttpError> = Ok("fine".into());
    assert_eq!(body_text(ok.into_response()).await, "fine");
    let bad: Result<String, HttpError> = Err(ExtractError::InvalidJson("nope".into()).into());
    let bad = bad.into_response();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    assert!(body_text(bad).await.contains("invalid JSON body"));
    let same = Response::ok();
    assert_eq!(same.into_response().status(), StatusCode::OK);
}

// ---- extractors -----------------------------------------------------------------

#[derive(Deserialize, Debug, PartialEq)]
struct Params {
    id: u32,
    name: String,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Search {
    q: String,
    page: Option<u32>,
}

#[tokio::test]
async fn path_parameters_are_typed() {
    let mut builder = Router::builder();
    builder.route(
        Method::GET,
        "/users/{id}/{name}",
        Arc::new(Probe(|mut req: Request| async move {
            match Path::<Params>::from_request(&mut req).await {
                Ok(Path(params)) => {
                    Response::text(StatusCode::OK, format!("{}:{}", params.id, params.name))
                }
                Err(err) => err.into_response(),
            }
        })),
    );
    let router = builder.finish();
    assert!(format!("{router:?}").contains("Router"));
    let c = container().await;
    let ok = router
        .dispatch(request(
            Method::GET,
            "/users/42/ann%20lee",
            HeaderMap::new(),
            "",
            &c,
        ))
        .await;
    assert_eq!(body_text(ok).await, "42:ann lee");
    let bad = router
        .dispatch(request(
            Method::GET,
            "/users/not-a-number/ann",
            HeaderMap::new(),
            "",
            &c,
        ))
        .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    assert!(body_text(bad).await.contains("invalid path parameter"));
}

#[tokio::test]
async fn query_strings_are_typed() {
    let mut req = simple("/s?q=rust+lang&page=2", "").await;
    let query = Query::<Search>::from_request(&mut req).await.unwrap();
    assert_eq!(
        *query,
        Search {
            q: "rust lang".into(),
            page: Some(2)
        }
    );
    assert_eq!(query.0.q, "rust lang");
    let mut req = simple("/s?q=x", "").await;
    assert_eq!(
        Query::<Search>::from_request(&mut req)
            .await
            .unwrap()
            .0
            .page,
        None
    );
    let mut req = simple("/s?page=nope", "").await;
    let err = Query::<Search>::from_request(&mut req).await.unwrap_err();
    assert_eq!(err.into_response().status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn json_bodies_are_typed_and_limited() {
    let mut req = simple("/", r#"{"id": 3, "name": "n"}"#).await;
    let body = Json::<Params>::from_request(&mut req).await.unwrap();
    assert_eq!(body.id, 3);
    let mut req = simple("/", "not json").await;
    let err = Json::<Params>::from_request(&mut req).await.unwrap_err();
    assert!(err.to_string().contains("invalid JSON body"));

    let small = Arc::new(
        ContainerBuilder::new()
            .provide(BodyLimit(8))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request(
        Method::POST,
        "/",
        HeaderMap::new(),
        r#"{"id": 3, "name": "far too long"}"#,
        &small,
    );
    let err = Json::<Params>::from_request(&mut req).await.unwrap_err();
    assert!(
        err.to_string().contains("exceeds the 8 byte limit"),
        "{err}"
    );
    assert_eq!(BodyLimit::default().0, 2 * 1024 * 1024);
}

#[tokio::test]
async fn form_bodies_are_typed() {
    let mut req = simple("/", "q=a+b&page=4").await;
    let form = Form::<Search>::from_request(&mut req).await.unwrap();
    assert_eq!(
        *form,
        Search {
            q: "a b".into(),
            page: Some(4)
        }
    );
    let mut req = simple("/", "page=x").await;
    let err = Form::<Search>::from_request(&mut req).await.unwrap_err();
    assert!(err.to_string().contains("invalid form body"));
}

#[tokio::test]
async fn raw_bytes_and_per_route_limits() {
    let mut req = simple("/", "raw bytes").await;
    let bytes = quench_http::extract::Bytes::from_request(&mut req)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"raw bytes");

    let mut req = simple("/", r#"{"id": 1, "name": "x"}"#).await;
    let limited = Limited::<Json<Params>, 100>::from_request(&mut req)
        .await
        .unwrap();
    assert_eq!(limited.0.0.id, 1);
    assert_eq!(limited.id, 1);
    let mut req = simple("/", r#"{"id": 1, "name": "x"}"#).await;
    assert!(
        Limited::<Json<Params>, 5>::from_request(&mut req)
            .await
            .is_err()
    );
    let mut req = simple("/", "not json at all").await;
    assert!(
        Limited::<Json<Params>, 100>::from_request(&mut req)
            .await
            .is_err()
    );

    let mut req = simple("/", "12345").await;
    let limited = Limited::<quench_http::extract::Bytes, 5>::from_request(&mut req)
        .await
        .unwrap();
    assert_eq!(limited.0.0.len(), 5);
    assert_eq!(limited.len(), 5);
    let mut req = simple("/", "123456").await;
    assert!(
        Limited::<quench_http::extract::Bytes, 5>::from_request(&mut req)
            .await
            .is_err()
    );
}

#[derive(Clone, Debug, PartialEq)]
struct Claims(&'static str);

#[derive(Debug)]
struct Service(u8);

#[tokio::test]
async fn extensions_and_injected_services() {
    let mut req = simple("/", "").await;
    req.extensions_mut().insert(Claims("alice"));
    assert_eq!(req.extensions().get::<Claims>(), Some(&Claims("alice")));
    let ext = Extension::<Claims>::from_request(&mut req).await.unwrap();
    assert_eq!(*ext, Claims("alice"));

    let mut missing = simple("/", "").await;
    let err = Extension::<Claims>::from_request(&mut missing)
        .await
        .unwrap_err();
    assert_eq!(
        err.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );

    let c = Arc::new(
        ContainerBuilder::new()
            .provide(Service(9))
            .build()
            .await
            .unwrap(),
    );
    let mut req = request(Method::GET, "/", HeaderMap::new(), "", &c);
    let injected = Inject::<Service>::from_request(&mut req).await.unwrap();
    assert_eq!(injected.0.0, 9);
    assert_eq!(injected.clone().0.0, 9);
    let mut none = simple("/", "").await;
    let err = Inject::<Service>::from_request(&mut none)
        .await
        .unwrap_err();
    assert_eq!(
        err.into_response().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[test]
fn extract_errors_name_their_cause() {
    for (err, needle) in [
        (
            ExtractError::MissingPathParam("id"),
            "missing path parameter `id`",
        ),
        (
            ExtractError::InvalidQuery("x".into()),
            "invalid query string",
        ),
        (
            ExtractError::InvalidMultipart("x".into()),
            "invalid multipart body",
        ),
        (ExtractError::MissingState("db"), "`db` is not available"),
    ] {
        assert!(err.to_string().contains(needle), "{err}");
    }
    assert!(
        quench_http::error::DiError::not_found::<String>()
            .to_string()
            .contains("String")
    );
}

// ---- requests -------------------------------------------------------------------

#[tokio::test]
async fn requests_expose_their_parts() {
    let c = container().await;
    let mut headers = HeaderMap::new();
    headers.insert("x-token", HeaderValue::from_static("abc"));
    headers.insert("x-binary", HeaderValue::from_bytes(b"caf\xe9").unwrap());
    let mut req = request(Method::PUT, "/a/b?c=d", headers, "payload", &c);
    assert_eq!(req.method(), Method::PUT);
    assert_eq!(req.uri().path(), "/a/b");
    assert_eq!(req.header("x-token"), Some("abc"));
    assert_eq!(
        req.header("x-binary"),
        None,
        "non-ASCII values read as absent"
    );
    assert_eq!(req.header("x-none"), None);
    assert_eq!(req.headers().len(), 2);
    assert_eq!(req.path_param("anything"), None);
    assert!(Arc::ptr_eq(req.container(), &c));
    assert_eq!(
        req.take_body().collect_limited(100).await.unwrap(),
        "payload"
    );
    assert_eq!(
        req.take_body().collect_limited(100).await.unwrap(),
        "",
        "the body can only be taken once"
    );
    assert!(format!("{req:?}").contains("Request"));
}

// ---- responses ------------------------------------------------------------------

#[tokio::test]
async fn responses_build_headers_and_bodies() {
    let response = Response::new(StatusCode::CREATED)
        .header("X-Mixed-Case", "v1")
        .header("bad name", "dropped")
        .header("x-bad-value", "line\nbreak")
        .append_header("Set-Cookie", "a=1")
        .append_header("set-cookie", "b=2")
        .append_header("bad name", "dropped")
        .append_header("x-bad", "nl\n");
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers().get("x-mixed-case").unwrap(), "v1");
    assert_eq!(response.headers().get_all("set-cookie").count(), 2);
    assert!(
        !response.headers().contains_key("bad name")
            && !response.headers().contains_key("x-bad-value")
            && !response.headers().contains_key("x-bad")
    );

    let cookie = quench_proto::cookie::Cookie::build(("sid", "tok"))
        .path("/")
        .http_only(true)
        .build();
    let with_cookie = Response::ok().cookie(&cookie);
    assert_eq!(
        with_cookie.headers().get("set-cookie").unwrap(),
        "sid=tok; HttpOnly; Path=/"
    );

    let html = Response::html(StatusCode::OK, "<p>hi</p>");
    assert_eq!(
        html.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    assert_eq!(body_text(html).await, "<p>hi</p>");
    let not_found = Response::not_found();
    assert_eq!(
        (not_found.status(), body_text(not_found).await),
        (StatusCode::NOT_FOUND, "not found".to_string())
    );
    let from_proto = Response::from(quench_proto::Response::new(StatusCode::ACCEPTED));
    assert_eq!(from_proto.status(), StatusCode::ACCEPTED);
    assert_eq!(Response::from(()).status(), StatusCode::NO_CONTENT);
    assert_eq!(body_text(Response::from(String::from("s"))).await, "s");
    assert_eq!(body_text(Response::from("lit")).await, "lit");
}

#[tokio::test]
async fn streaming_and_event_responses() {
    let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
        Ok(Bytes::from_static(b"one")),
        Ok(Bytes::from_static(b"two")),
    ];
    let streamed = Response::streaming(StatusCode::OK, stream::iter(chunks));
    assert_eq!(body_text(streamed).await, "onetwo");
    let sse = Response::sse(stream::iter(vec![
        quench_proto::sse::Event::new().data("tick"),
    ]));
    assert_eq!(
        sse.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
    assert_eq!(body_text(sse).await, "data: tick\n\n");
    let value = Response::json(StatusCode::OK, &json!({"a": [1, 2]})).unwrap();
    assert_eq!(body_text(value).await, r#"{"a":[1,2]}"#);
}

// ---- routing --------------------------------------------------------------------

#[tokio::test]
async fn routers_pick_by_path_then_method() {
    let ok = |label: &'static str| {
        Arc::new(Probe(move |_req: Request| async move {
            Response::text(StatusCode::OK, label)
        })) as Arc<dyn Endpoint>
    };
    let mut builder = Router::builder();
    builder
        .route(Method::GET, "/thing", ok("get"))
        .route(Method::POST, "/thing", ok("post"))
        .route(Method::GET, "/files/{rest:.+}", ok("file"));
    let router = builder.finish();
    let c = container().await;
    let call = |method: Method, path: &'static str| {
        let c = c.clone();
        let router = &router;
        async move {
            router
                .call(request(method, path, HeaderMap::new(), "", &c))
                .await
        }
    };
    assert_eq!(body_text(call(Method::GET, "/thing").await).await, "get");
    assert_eq!(body_text(call(Method::POST, "/thing").await).await, "post");
    assert_eq!(
        call(Method::DELETE, "/thing").await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        call(Method::GET, "/nowhere").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        body_text(call(Method::GET, "/files/a/b/c").await).await,
        "file"
    );
}

#[tokio::test]
async fn mounted_routers_see_paths_without_the_prefix() {
    let inner = Arc::new(Probe(|req: Request| async move {
        Response::text(StatusCode::OK, req.uri().path().to_string())
    })) as Arc<dyn Endpoint>;
    let mut builder = Router::builder();
    builder.route(Method::GET, "/ping", inner);
    let mounted = mount("/svc", Arc::new(builder.finish()));
    let c = container().await;
    assert_eq!(
        mounted
            .call(request(Method::GET, "/svc/ping", HeaderMap::new(), "", &c))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        mounted
            .call(request(
                Method::GET,
                "/other/ping",
                HeaderMap::new(),
                "",
                &c
            ))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        mounted
            .call(request(Method::GET, "/svcx/ping", HeaderMap::new(), "", &c))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let at_root = mount("/", Arc::new(Probe(|_r: Request| async { Response::ok() })));
    assert_eq!(
        at_root
            .call(request(Method::GET, "/anything", HeaderMap::new(), "", &c))
            .await
            .status(),
        StatusCode::OK
    );
}

// ---- servers --------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn serve_http_stops_on_sigterm() {
    let app: Arc<dyn Endpoint> = Arc::new(Probe(|_req: Request| async { Response::ok() }));
    let server = tokio::spawn(quench_http::server::serve_http(
        app,
        container().await,
        "127.0.0.1:0".parse().unwrap(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!server.is_finished());
    // The handler is registered by now, so the signal is consumed instead of killing the test process.
    let status = std::process::Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .expect("the server drains and returns");
    result.unwrap().unwrap();
}

#[cfg(feature = "tls")]
#[tokio::test]
async fn serve_https_binds_and_load_tls_rejects_missing_files() {
    assert!(
        quench_http::server::load_tls("/nonexistent/cert.pem", "/nonexistent/key.pem").is_none()
    );
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../quench-tls/tests/fixtures");
    let config =
        quench_http::server::load_tls(format!("{dir}/server.pem"), format!("{dir}/server.key"))
            .unwrap();
    let app: Arc<dyn Endpoint> = Arc::new(Probe(|_req: Request| async { Response::ok() }));
    let server = tokio::spawn(quench_http::server::serve_https(
        app,
        container().await,
        "127.0.0.1:0".parse().unwrap(),
        config,
    ));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!server.is_finished());
    server.abort();
}

#[tokio::test]
async fn binding_a_taken_address_is_an_error() {
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = taken.local_addr().unwrap();
    let app: Arc<dyn Endpoint> = Arc::new(Probe(|_req: Request| async { Response::ok() }));
    assert!(
        quench_http::server::serve_http(app, container().await, addr)
            .await
            .is_err()
    );
}
