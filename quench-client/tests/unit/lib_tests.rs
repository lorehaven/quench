//! Unit tests for `lib.rs`, exercised against the raw-HTTP test double in
//! `mock_server.rs` (no live network access, no mocking crate dependency).

use quench_client::{BasicAuthClient, BearerAuthClient, ClientCredentialsClient, HttpClient};
use serde_json::{Value, json};

use crate::mock_server::{serve_n, serve_once};

fn lowercased(head: &str) -> String {
    head.to_lowercase()
}

#[tokio::test]
async fn http_client_get_sends_a_plain_get_and_parses_the_json_body() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = HttpClient::new(&base_url).expect("build client");

    let value: Value = client.get("/things/1").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));

    let captured = handle.await.expect("server task");
    assert_eq!(captured.method, "GET");
    assert_eq!(captured.path, "/things/1");
}

#[tokio::test]
async fn http_client_post_put_and_delete_send_their_method_and_json_body() {
    for (method, body_sent) in [
        ("POST", json!({"name": "a"})),
        ("PUT", json!({"name": "b"})),
    ] {
        let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
        let client = HttpClient::new(&base_url).expect("build client");

        let value: Value = if method == "POST" {
            client.post("/things", &body_sent).await
        } else {
            client.put("/things/1", &body_sent).await
        }
        .expect("request should succeed");
        assert_eq!(value, json!({"ok": true}));

        let captured = handle.await.expect("server task");
        assert_eq!(captured.method, method);
        assert!(captured.body.contains("\"name\""));
    }

    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = HttpClient::new(&base_url).expect("build client");
    let value: Value = client.delete("/things/1").await.expect("delete succeeds");
    assert_eq!(value, json!({"ok": true}));
    let captured = handle.await.expect("server task");
    assert_eq!(captured.method, "DELETE");
}

#[tokio::test]
async fn http_client_surfaces_the_status_and_body_on_a_non_success_response() {
    let (base_url, handle) = serve_once("404 Not Found", "no such thing").await;
    let client = HttpClient::new(&base_url).expect("build client");

    let err = client
        .get::<Value>("/missing")
        .await
        .expect_err("404 should be an error");
    let message = format!("{err:#}");
    assert!(message.contains("404"));
    assert!(message.contains("no such thing"));

    handle.await.expect("server task");
}

#[tokio::test]
async fn http_client_builder_can_disable_tls_verification() {
    let client = HttpClient::builder("http://localhost")
        .tls_verify(false)
        .build();
    assert!(client.is_ok());
}

#[tokio::test]
async fn basic_auth_client_sends_the_basic_authorization_header() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");

    let value: Value = client.get("/me").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));

    let captured = handle.await.expect("server task");
    assert!(lowercased(&captured.head).contains("authorization: basic"));
}

#[tokio::test]
async fn basic_auth_client_builder_supports_every_combinator() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::builder(&base_url)
        .username("bob")
        .password("hunter2")
        .tls_verify(true)
        .build()
        .expect("build client");

    let posted: Value = client
        .post("/things", &json!({"n": 1}))
        .await
        .expect("post should succeed");
    assert_eq!(posted, json!({"ok": true}));
    handle.await.expect("server task");

    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::builder(&base_url)
        .username("bob")
        .password("hunter2")
        .build()
        .expect("build client");
    let updated: Value = client
        .put("/things/1", &json!({"n": 2}))
        .await
        .expect("put should succeed");
    assert_eq!(updated, json!({"ok": true}));
    handle.await.expect("server task");
}

#[tokio::test]
async fn basic_auth_client_delete_expect_success_ignores_a_2xx_body() {
    let (base_url, handle) = serve_once("204 No Content", "").await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");

    client
        .delete_expect_success("/things/1")
        .await
        .expect("204 should be treated as success");
    handle.await.expect("server task");
}

#[tokio::test]
async fn basic_auth_client_delete_expect_success_surfaces_a_failure_status() {
    let (base_url, handle) = serve_once("500 Internal Server Error", "boom").await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");

    let err = client
        .delete_expect_success("/things/1")
        .await
        .expect_err("500 should be an error");
    let message = format!("{err:#}");
    assert!(message.contains("500"));
    assert!(message.contains("boom"));
    handle.await.expect("server task");
}

#[tokio::test]
async fn basic_auth_client_delete_returns_the_parsed_body_on_success() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");
    let value: Value = client.delete("/things/1").await.expect("delete succeeds");
    assert_eq!(value, json!({"ok": true}));
    handle.await.expect("server task");
}

#[tokio::test]
async fn bearer_auth_client_sends_the_bearer_authorization_header() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BearerAuthClient::new(&base_url, "tok-123").expect("build client");

    let value: Value = client.get("/me").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));
    let captured = handle.await.expect("server task");
    assert!(lowercased(&captured.head).contains("bearer tok-123"));
}

#[tokio::test]
async fn bearer_auth_client_with_tls_verify_false_still_posts_correctly() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client =
        BearerAuthClient::with_tls_verify(&base_url, "tok-123", false).expect("build client");

    let value: Value = client
        .post("/things", &json!({"n": 1}))
        .await
        .expect("post should succeed");
    assert_eq!(value, json!({"ok": true}));
    let captured = handle.await.expect("server task");
    assert_eq!(captured.method, "POST");
}

#[tokio::test]
async fn client_credentials_client_exchanges_a_token_then_calls_the_target_with_it() {
    let (token_url, token_handle) = serve_once(
        "200 OK",
        r#"{"access_token":"exchanged","expires_in":3600}"#,
    )
    .await;
    let (base_url, api_handle) = serve_once("200 OK", r#"{"ok":true}"#).await;

    let client = ClientCredentialsClient::builder(&base_url)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("shh")
        .tls_verify(true)
        .build()
        .expect("build client");

    let value: Value = client.get("/things/1").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));

    let token_request = token_handle.await.expect("token server task");
    assert_eq!(token_request.method, "POST");
    assert!(token_request.body.contains("grant_type=client_credentials"));
    assert!(token_request.body.contains("client_id=svc"));

    let api_request = api_handle.await.expect("api server task");
    assert!(lowercased(&api_request.head).contains("bearer exchanged"));
}

#[tokio::test]
async fn client_credentials_client_caches_the_token_across_calls() {
    // The token endpoint is only wired to accept ONE connection - if
    // `access_token` mistakenly re-exchanged on the second `get`, that
    // second POST would hang waiting for a listener that is no longer
    // accepting, and the test would time out instead of passing.
    let (token_url, mut token_rx) = serve_n(
        1,
        "200 OK",
        r#"{"access_token":"cached-tok","expires_in":3600}"#,
    )
    .await;
    let (api_url, mut api_rx) = serve_n(2, "200 OK", r#"{"ok":true}"#).await;

    let client = ClientCredentialsClient::builder(&api_url)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("shh")
        .build()
        .expect("build client");

    let first: Value = client.get("/a").await.expect("first get succeeds");
    assert_eq!(first, json!({"ok": true}));
    let second: Value = client.get("/b").await.expect("second get succeeds");
    assert_eq!(second, json!({"ok": true}));

    let token_request = token_rx
        .recv()
        .await
        .expect("token server hit exactly once");
    assert_eq!(token_request.method, "POST");

    let first_api = api_rx.recv().await.expect("first api call captured");
    let second_api = api_rx.recv().await.expect("second api call captured");
    for request in [&first_api, &second_api] {
        assert!(lowercased(&request.head).contains("bearer cached-tok"));
    }
    assert_eq!(first_api.path, "/a");
    assert_eq!(second_api.path, "/b");
}

#[tokio::test]
async fn client_credentials_client_post_exchanges_a_token_and_sends_the_body() {
    let (token_url, token_handle) = serve_once(
        "200 OK",
        r#"{"access_token":"posted-tok","expires_in":3600}"#,
    )
    .await;
    let (api_url, api_handle) = serve_once("200 OK", r#"{"ok":true}"#).await;

    let client = ClientCredentialsClient::builder(&api_url)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("shh")
        .build()
        .expect("build client");

    let value: Value = client
        .post("/things", &json!({"n": 1}))
        .await
        .expect("post should succeed");
    assert_eq!(value, json!({"ok": true}));

    token_handle.await.expect("token server task");
    let api_request = api_handle.await.expect("api server task");
    assert_eq!(api_request.method, "POST");
    assert!(lowercased(&api_request.head).contains("bearer posted-tok"));
    assert!(api_request.body.contains("\"n\""));
}

#[tokio::test]
async fn client_credentials_client_surfaces_a_refused_token_exchange() {
    let (token_url, token_handle) = serve_once("401 Unauthorized", "bad client secret").await;
    let client = ClientCredentialsClient::builder("http://127.0.0.1:1")
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("wrong")
        .build()
        .expect("build client");

    let err = client
        .get::<Value>("/a")
        .await
        .expect_err("a refused token exchange should error out");
    let message = format!("{err:#}");
    assert!(message.contains("client_credentials grant was refused"));
    assert!(message.contains("bad client secret"));

    token_handle.await.expect("token server task");
}

#[tokio::test]
async fn client_credentials_client_delete_expect_success_covers_both_branches() {
    let (token_url, _token_handle) =
        serve_once("200 OK", r#"{"access_token":"tok","expires_in":3600}"#).await;
    let (api_url, api_handle) = serve_once("204 No Content", "").await;

    let client = ClientCredentialsClient::builder(&api_url)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("shh")
        .build()
        .expect("build client");

    client
        .delete_expect_success("/things/1")
        .await
        .expect("204 should be treated as success");
    api_handle.await.expect("api server task");

    let (token_url, _token_handle) =
        serve_once("200 OK", r#"{"access_token":"tok","expires_in":3600}"#).await;
    let (api_url, api_handle) = serve_once("500 Internal Server Error", "nope").await;
    let client = ClientCredentialsClient::builder(&api_url)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("shh")
        .build()
        .expect("build client");

    let err = client
        .delete_expect_success("/things/1")
        .await
        .expect_err("500 should be an error");
    let message = format!("{err:#}");
    assert!(message.contains("500"));
    assert!(message.contains("nope"));
    api_handle.await.expect("api server task");
}
