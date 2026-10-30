#![cfg_attr(not(feature = "tls"), allow(unused_imports))] // the https tests carry most of the TLS imports
//! `quench_client::transport` against a real quench-http server, over plain
//! HTTP and over TLS with a self-signed certificate (`tests/fixtures`).

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_client::transport::{Client, Error};
use quench_core::async_trait;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, Method, StatusCode};
use quench_http::request::Request;
use quench_http::response::Response;
use quench_http::server::{load_tls, serve_http_on, serve_https_on};
use quench_serde::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

struct App;

#[async_trait]
impl Endpoint for App {
    async fn call(&self, mut req: Request) -> Response {
        let path = req.uri().path().to_string();
        match path.as_str() {
            "/redirect-get" | "/same-origin" => return redirect(StatusCode::FOUND, "/echo"),
            "/redirect-keep" => return redirect(StatusCode::TEMPORARY_REDIRECT, "/echo"),
            "/cross-origin" => {
                let host = req
                    .headers()
                    .get("host")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or_default()
                    .replace("127.0.0.1", "localhost");
                return redirect(StatusCode::FOUND, &format!("http://{host}/echo"));
            }
            "/loop" => return redirect(StatusCode::FOUND, "/loop"),
            "/cookies" => {
                return Response::ok()
                    .append_header("set-cookie", "session=abc123; Path=/; HttpOnly")
                    .append_header("set-cookie", "theme = dark ; Secure");
            }
            "/stream" => {
                let chunks = quench_core::stream::iter(vec![
                    Ok::<_, std::io::Error>(quench_core::bytes::Bytes::from_static(b"one,")),
                    Ok(quench_core::bytes::Bytes::from_static(b"two,")),
                    Ok(quench_core::bytes::Bytes::from_static(b"three")),
                ]);
                return Response::streaming(StatusCode::OK, chunks);
            }
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
                "cookie": header("cookie"),
                "user_agent": header("user-agent"),
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

#[cfg(feature = "tls")]
async fn tls_server() -> String {
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn credentials_follow_a_redirect_only_within_the_same_origin() {
    let base = plain_server().await;
    let client = Client::new();

    for (path, kept) in [("same-origin", true), ("cross-origin", false)] {
        let got: Value = client
            .get(format!("{base}/{path}"))
            .header("authorization", "Bearer secret")
            .header("cookie", "session=abc")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(got["authorization"].is_string(), kept, "{path}: {got}");
        assert_eq!(got["cookie"].is_string(), kept, "{path}: {got}");
    }
}

#[quench_test::test]
async fn the_redirect_policy_can_hand_back_the_redirect() {
    use quench_client::transport::Redirect;
    let base = plain_server().await;

    let client = Client::builder().redirect(Redirect::None).build().unwrap();
    let response = client
        .get(format!("{base}/redirect-get"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok()),
        Some("/echo")
    );

    let limited = Client::builder()
        .redirect(Redirect::Limited(2))
        .build()
        .unwrap();
    let err = limited
        .get(format!("{base}/loop"))
        .send()
        .await
        .err()
        .unwrap();
    assert!(matches!(err, Error::Transport(m) if m.contains("redirect")));
    let followed = limited
        .get(format!("{base}/redirect-get"))
        .send()
        .await
        .unwrap();
    assert_eq!(followed.status(), StatusCode::OK);
}

#[quench_test::test]
async fn multipart_bodies_carry_their_boundary() {
    use quench_client::multipart::{Form, Part};
    let base = plain_server().await;
    let form = Form::new().text("title", "hello").part(
        "file",
        Part::bytes(&b"contents"[..])
            .file_name("a.txt")
            .mime_str("text/plain"),
    );
    let boundary = form.boundary().to_string();
    let echoed: Value = Client::new()
        .post(format!("{base}/echo"))
        .multipart(form)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        echoed["content_type"],
        format!("multipart/form-data; boundary={boundary}").as_str()
    );
    let body = echoed["body"].as_str().unwrap();
    assert!(body.contains("name=\"title\"\r\n\r\nhello"));
    assert!(body.contains("filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\ncontents"));
}

#[quench_test::test]
async fn cookies_are_read_from_responses_and_sent_back() {
    let base = plain_server().await;
    let client = Client::new();
    let response = client.get(format!("{base}/cookies")).send().await.unwrap();
    assert_eq!(
        response.cookies(),
        vec![
            ("session".to_string(), "abc123".to_string()),
            ("theme".to_string(), "dark".to_string())
        ]
    );
    assert_eq!(response.cookie("theme").as_deref(), Some("dark"));
    assert_eq!(response.cookie("missing"), None);

    let echoed: Value = client
        .get(format!("{base}/echo"))
        .cookies(&[("session", "abc123"), ("theme", "dark")])
        .cookies::<&str, &str>(&[])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["cookie"], "session=abc123; theme=dark");
}

#[quench_test::test]
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

#[quench_test::test]
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

#[cfg(not(feature = "tls"))]
#[quench_test::test]
async fn https_is_refused_without_the_tls_feature() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let error = Client::new()
        .get(format!("https://127.0.0.1:{port}/"))
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Transport(m) if m.contains("`tls` feature")),
        "{error}"
    );
}

#[cfg(feature = "tls")]
#[quench_test::test]
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

// ---- pooling, TLS roots, limits ------------------------------------------------

mod pool {
    use super::{App, Client, Error};
    use quench_core::async_trait;
    use quench_http::di::ContainerBuilder;
    use quench_http::endpoint::Endpoint;
    use quench_http::request::Request;
    use quench_http::response::Response;
    use quench_http::server::serve_https_on;
    use quench_proto::StatusCode;
    use quench_proto::h1::server::{ServerConfig, Shutdown, serve_connection};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves `App` and counts how many connections were opened.
    async fn counting_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            struct Echo;
            #[async_trait]
            impl Endpoint for Echo {
                async fn call(&self, req: Request) -> Response {
                    match req.uri().path() {
                        "/big" => Response::text(StatusCode::OK, "x".repeat(10_000)),
                        "/to-hello" => Response::new(StatusCode::FOUND)
                            .header("location", "hello?from=redirect"),
                        "/hello" => Response::text(StatusCode::OK, "hello"),
                        _ => Response::text(StatusCode::OK, "ok"),
                    }
                }
            }
            let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                let container = container.clone();
                tokio::spawn(async move {
                    struct Svc(Arc<quench_http::di::Container>);
                    impl quench_proto::h1::server::Service for Svc {
                        fn call(
                            &self,
                            req: quench_proto::Request,
                        ) -> quench_core::future::BoxFuture<'_, quench_proto::Response>
                        {
                            Box::pin(async move {
                                let quench_proto::Request {
                                    method,
                                    uri,
                                    headers,
                                    body,
                                    ..
                                } = req;
                                let request = Request::new(
                                    method,
                                    uri,
                                    headers,
                                    quench_http::body::InboundBody::from(body),
                                    self.0.clone(),
                                );
                                Echo.call(request).await.into_inner()
                            })
                        }
                    }
                    let _ = serve_connection(
                        stream,
                        &Svc(container),
                        &ServerConfig::default(),
                        Shutdown::never(),
                    )
                    .await;
                });
            }
        });
        (format!("http://{addr}"), accepted)
    }

    #[quench_test::test]
    async fn sequential_requests_share_one_connection() {
        let (base, accepted) = counting_server().await;
        let client = Client::new();
        for _ in 0..5 {
            let text = client
                .get(format!("{base}/hello"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            assert_eq!(text, "hello");
        }
        assert_eq!(accepted.load(Ordering::SeqCst), 1, "keep-alive reuse");
    }

    #[quench_test::test]
    async fn redirects_reuse_the_connection_and_resolve_relative_targets() {
        let (base, accepted) = counting_server().await;
        let response = Client::new()
            .get(format!("{base}/to-hello#ignored-fragment"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "hello");
        assert_eq!(accepted.load(Ordering::SeqCst), 1);
    }

    #[quench_test::test]
    async fn a_connection_the_server_closed_while_idle_is_replaced() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    // A keep-alive response, then the socket is closed behind the client's back.
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                        .await
                        .unwrap();
                });
            }
        });
        let client = Client::new();
        for _ in 0..3 {
            assert_eq!(
                client
                    .get(format!("http://{addr}/"))
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap(),
                "ok"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(
            accepted.load(Ordering::SeqCst),
            3,
            "dead pooled connections are noticed, not reused"
        );
    }

    #[quench_test::test]
    async fn a_request_that_dies_on_a_reused_connection_is_retried_once() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                        .await
                        .unwrap();
                    // A second request on this connection is read and then hung up on, unanswered.
                    let _ = stream.read(&mut buf).await;
                });
            }
        });
        let client = Client::new();
        assert_eq!(
            client
                .get(format!("http://{addr}/"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "ok"
        );
        // The first connection is pooled and looks alive; the server drops it on use.
        assert_eq!(
            client
                .get(format!("http://{addr}/"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "ok"
        );
        assert_eq!(accepted.load(Ordering::SeqCst), 2);
    }

    #[quench_test::test]
    async fn non_idempotent_requests_are_not_replayed_on_a_dead_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    stream
                        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                        .await
                        .unwrap();
                    let _ = stream.read(&mut buf).await;
                });
            }
        });
        let client = Client::new();
        client.get(format!("http://{addr}/")).send().await.unwrap();
        let err = client
            .post(format!("http://{addr}/"))
            .body("x")
            .send()
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Transport(_)), "{err:?}");
    }

    #[quench_test::test]
    async fn response_size_limits_are_enforced() {
        let (base, _) = counting_server().await;
        let limited = Client::builder().max_response_bytes(100).build().unwrap();
        let err = limited
            .get(format!("{base}/big"))
            .send()
            .await
            .err()
            .unwrap();
        assert!(
            matches!(&err, Error::Transport(m) if m.contains("exceeds")),
            "{err:?}"
        );
        assert_eq!(
            limited
                .get(format!("{base}/hello"))
                .send()
                .await
                .unwrap()
                .bytes()
                .await
                .unwrap(),
            "hello"
        );
    }

    #[quench_test::test]
    async fn bad_urls_are_request_errors() {
        let client = Client::new();
        for url in [
            "ftp://example.com/file",
            "/relative/only",
            "http://",
            "http://host:notaport/",
        ] {
            let err = client.get(url).send().await.err().unwrap();
            assert!(matches!(err, Error::Request(_)), "{url}: {err:?}");
        }
        let err = client
            .get("http://example.com/")
            .header("bad name", "v")
            .send()
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Request(m) if m.contains("invalid header")));
        let err = client
            .post("http://example.com/")
            .form(&1u8)
            .send()
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Request(_)));
    }

    #[quench_test::test]
    async fn raw_bodies_and_text_decoding() {
        let (base, _) = counting_server().await;
        let response = Client::new()
            .put(format!("{base}/anything"))
            .body(&b"raw"[..])
            .header("content-type", "application/octet-stream")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(response.text().await.unwrap(), "ok");
        let deleted = Client::new()
            .delete(format!("{base}/anything"))
            .send()
            .await
            .unwrap();
        assert!(deleted.status().is_success());
        assert!(
            format!("{:?}", Client::new().get("http://x/").bearer_auth("secret"))
                .contains("authorization")
        );
        assert!(
            !format!("{:?}", Client::new().get("http://x/").bearer_auth("secret"))
                .contains("secret")
        );
    }

    #[cfg(feature = "tls")]
    #[quench_test::test]
    async fn a_private_ca_can_be_trusted_explicitly() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../quench-tls/tests/fixtures");
        let config =
            quench_http::server::load_tls(format!("{dir}/server.pem"), format!("{dir}/server.key"))
                .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        tokio::spawn(serve_https_on(
            Arc::new(App),
            container,
            listener,
            config,
            std::future::pending(),
        ));

        let ca = std::fs::read(format!("{dir}/ca.pem")).unwrap();
        let trusting = Client::builder()
            .add_root_certificate_pem(ca)
            .build()
            .unwrap();
        let response = trusting
            .get(format!("https://localhost:{port}/echo"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        // Same server, different client without the CA: rejected.
        assert!(
            Client::new()
                .get(format!("https://localhost:{port}/echo"))
                .send()
                .await
                .is_err()
        );
        // Right CA, wrong name: rejected.
        assert!(
            trusting
                .get(format!("https://127.0.0.2:{port}/echo"))
                .send()
                .await
                .is_err()
        );
    }

    #[quench_test::test]
    async fn ipv6_literals_are_connectable() {
        let Ok(listener) = TcpListener::bind("[::1]:0").await else {
            return;
        };
        let port = listener.local_addr().unwrap().port();
        let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
        tokio::spawn(quench_http::server::serve_http_on(
            Arc::new(App),
            container,
            listener,
            std::future::pending(),
        ));
        let response = Client::new()
            .get(format!("http://[::1]:{port}/echo"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
}

#[quench_test::test]
async fn a_client_user_agent_is_sent_and_a_request_can_replace_it() {
    let base = plain_server().await;
    let client = Client::builder().user_agent("conveyor/1").build().unwrap();
    let echoed: Value = client
        .get(format!("{base}/echo"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["user_agent"], "conveyor/1");
    let replaced: Value = client
        .get(format!("{base}/echo"))
        .header("user-agent", "other")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(replaced["user_agent"], "other");
    let none: Value = Client::new()
        .get(format!("{base}/echo"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(none["user_agent"], Value::Null);
    assert!(matches!(
        Client::builder().user_agent("bad\nagent").build(),
        Err(Error::Build(_))
    ));
}

#[quench_test::test]
async fn a_streamed_response_arrives_chunk_by_chunk_or_whole() {
    use quench_core::stream::StreamExt;
    let base = plain_server().await;
    let client = Client::new();

    let response = client
        .get(format!("{base}/stream"))
        .send_streaming()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.bytes_stream();
    let mut all = Vec::new();
    while let Some(chunk) = stream.next().await {
        all.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(all, b"one,two,three");

    // The ordinary readers work on a streamed response too, and a buffered one is a single chunk.
    let text = client
        .get(format!("{base}/stream"))
        .send_streaming()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(text, "one,two,three");
    let mut buffered = client
        .get(format!("{base}/echo"))
        .send()
        .await
        .unwrap()
        .bytes_stream();
    assert!(buffered.next().await.unwrap().is_ok());
    assert!(buffered.next().await.is_none());

    let capped = Client::builder().max_response_bytes(5).build().unwrap();
    let error = capped
        .get(format!("{base}/stream"))
        .send_streaming()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Transport(_)), "{error}");
    assert!(
        Client::new()
            .get("not a url")
            .send_streaming()
            .await
            .is_err()
    );
}

#[quench_test::test]
async fn headers_can_be_added_in_bulk_and_the_protocol_types_come_with_the_client() {
    use quench_client::transport::{HeaderMap, HeaderValue, header};
    let base = plain_server().await;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_static("Bearer bulk"),
    );
    let echoed: Value = Client::new()
        .get(format!("{base}/echo"))
        .headers(headers)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["authorization"], "Bearer bulk");
}

#[quench_test::test]
async fn patch_and_head_are_requests_too() {
    let base = plain_server().await;
    let client = Client::new();
    let patched: Value = client
        .patch(format!("{base}/echo"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(patched["method"], "PATCH");
    let head = client.head(format!("{base}/echo")).send().await.unwrap();
    assert!(head.status().is_success());
}

#[quench_test::test]
async fn query_pairs_are_appended_and_encoded() {
    let base = plain_server().await;
    let client = Client::new();
    let echoed: Value = client
        .get(format!("{base}/echo"))
        .query(&[("q", "a b&c"), ("n", "1")])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(echoed["path"], "/echo");
    let again = client.get(format!("{base}/echo?x=1")).query(&[("y", "2")]);
    assert!(format!("{again:?}").contains("x=1&y=2"), "{again:?}");
    let untouched = client.get(format!("{base}/echo")).query::<&str, &str>(&[]);
    assert!(!format!("{untouched:?}").contains('?'));
    let bad = Client::new().get("not a url").query(&[("a", "b")]);
    assert!(bad.send().await.is_err());
}

#[cfg(feature = "tls")]
#[quench_test::test]
#[allow(clippy::redundant_clone)] // the blocking half below needs its own copy only with that feature
fn a_client_identity_is_accepted_or_refused_at_build_time() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let cert = std::fs::read(format!("{dir}/client.pem")).unwrap();
    let key = std::fs::read(format!("{dir}/client.key")).unwrap();
    assert!(
        Client::builder()
            .client_identity_pem(cert.clone(), key.clone())
            .build()
            .is_ok()
    );
    let error = Client::builder()
        .client_identity_pem(cert, b"not a key".to_vec())
        .build()
        .err()
        .unwrap();
    assert!(matches!(error, Error::Build(m) if m.contains("private key")));
    #[cfg(feature = "blocking")]
    assert!(
        quench_client::blocking::Client::builder()
            .client_identity_pem(std::fs::read(format!("{dir}/client.pem")).unwrap(), key)
            .build()
            .is_ok()
    );
}

quench_test::main!();
