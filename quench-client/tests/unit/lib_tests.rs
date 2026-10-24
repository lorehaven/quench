//! Unit tests for `lib.rs`, exercised against the raw-HTTP test double in
//! `mock_server.rs` (no live network access, no mocking crate dependency).

use quench_client::{BasicAuthClient, BearerAuthClient, ClientCredentialsClient, HttpClient};
use quench_serde::{Value, json};

use crate::mock_server::{serve_n, serve_once};

fn lowercased(head: &str) -> String {
    head.to_lowercase()
}

#[quench_test::test]
async fn http_client_get_sends_a_plain_get_and_parses_the_json_body() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = HttpClient::new(&base_url).expect("build client");

    let value: Value = client.get("/things/1").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));

    let captured = handle.await.expect("server task");
    assert_eq!(captured.method, "GET");
    assert_eq!(captured.path, "/things/1");
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
async fn http_client_builder_can_disable_tls_verification() {
    let client = HttpClient::builder("http://localhost")
        .tls_verify(false)
        .build();
    assert!(client.is_ok());
}

#[quench_test::test]
async fn basic_auth_client_sends_the_basic_authorization_header() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");

    let value: Value = client.get("/me").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));

    let captured = handle.await.expect("server task");
    assert!(lowercased(&captured.head).contains("authorization: basic"));
}

#[quench_test::test]
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

#[quench_test::test]
async fn basic_auth_client_delete_expect_success_ignores_a_2xx_body() {
    let (base_url, handle) = serve_once("204 No Content", "").await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");

    client
        .delete_expect_success("/things/1")
        .await
        .expect("204 should be treated as success");
    handle.await.expect("server task");
}

#[quench_test::test]
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

#[quench_test::test]
async fn basic_auth_client_delete_returns_the_parsed_body_on_success() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BasicAuthClient::new(&base_url, "alice", "secret").expect("build client");
    let value: Value = client.delete("/things/1").await.expect("delete succeeds");
    assert_eq!(value, json!({"ok": true}));
    handle.await.expect("server task");
}

#[quench_test::test]
async fn bearer_auth_client_sends_the_bearer_authorization_header() {
    let (base_url, handle) = serve_once("200 OK", r#"{"ok":true}"#).await;
    let client = BearerAuthClient::new(&base_url, "tok-123").expect("build client");

    let value: Value = client.get("/me").await.expect("get should succeed");
    assert_eq!(value, json!({"ok": true}));
    let captured = handle.await.expect("server task");
    assert!(lowercased(&captured.head).contains("bearer tok-123"));
}

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

#[quench_test::test]
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

const DEAD: &str = "http://127.0.0.1:1";

#[quench_test::test]
async fn every_client_reports_a_connection_failure_with_context() {
    let http = HttpClient::new(DEAD).unwrap();
    for message in [
        format!("{:#}", http.get::<Value>("/x").await.unwrap_err()),
        format!(
            "{:#}",
            http.post::<_, Value>("/x", &json!({})).await.unwrap_err()
        ),
        format!(
            "{:#}",
            http.put::<_, Value>("/x", &json!({})).await.unwrap_err()
        ),
        format!("{:#}", http.delete::<Value>("/x").await.unwrap_err()),
    ] {
        assert!(message.contains("Failed to send"), "{message}");
    }

    let basic = BasicAuthClient::new(DEAD, "user", "pw").unwrap();
    for message in [
        format!("{:#}", basic.get::<Value>("/x").await.unwrap_err()),
        format!(
            "{:#}",
            basic.post::<_, Value>("/x", &json!({})).await.unwrap_err()
        ),
        format!(
            "{:#}",
            basic.put::<_, Value>("/x", &json!({})).await.unwrap_err()
        ),
        format!("{:#}", basic.delete::<Value>("/x").await.unwrap_err()),
        format!("{:#}", basic.delete_expect_success("/x").await.unwrap_err()),
    ] {
        assert!(message.contains("Failed to send"), "{message}");
    }

    let bearer = BearerAuthClient::new(DEAD, "tok").unwrap();
    assert!(
        format!("{:#}", bearer.get::<Value>("/x").await.unwrap_err())
            .contains("Failed to send GET")
    );
    assert!(
        format!(
            "{:#}",
            bearer.post::<_, Value>("/x", &json!({})).await.unwrap_err()
        )
        .contains("Failed to send POST")
    );

    let credentials = ClientCredentialsClient::builder(DEAD)
        .token_url(&format!("{DEAD}/token"))
        .client_id("svc")
        .client_secret("s")
        .build()
        .unwrap();
    let message = format!("{:#}", credentials.get::<Value>("/x").await.unwrap_err());
    assert!(message.contains("token request failed"), "{message}");
    assert!(
        credentials
            .post::<_, Value>("/x", &json!({}))
            .await
            .is_err()
    );
    assert!(credentials.delete_expect_success("/x").await.is_err());
}

#[quench_test::test]
async fn an_unparseable_token_response_is_reported() {
    let (token_url, handle) = serve_once("200 OK", "this is not json").await;
    let client = ClientCredentialsClient::builder(DEAD)
        .token_url(&format!("{token_url}/token"))
        .client_id("svc")
        .client_secret("s")
        .build()
        .unwrap();
    let message = format!("{:#}", client.get::<Value>("/x").await.unwrap_err());
    assert!(
        message.contains("failed to parse the client_credentials token response"),
        "{message}"
    );
    handle.await.unwrap();
}

#[quench_test::test]
async fn error_statuses_carry_the_response_body() {
    let (base, handle) = serve_once("500 Internal Server Error", "kaput").await;
    let message = format!(
        "{:#}",
        HttpClient::new(&base)
            .unwrap()
            .get::<Value>("/x")
            .await
            .unwrap_err()
    );
    assert!(
        message.contains("500") && message.contains("kaput"),
        "{message}"
    );
    handle.await.unwrap();
    let (base, handle) = serve_once("404 Not Found", "gone").await;
    let message = format!(
        "{:#}",
        BasicAuthClient::new(&base, "u", "p")
            .unwrap()
            .delete_expect_success("/x")
            .await
            .unwrap_err()
    );
    assert!(
        message.contains("404") && message.contains("gone"),
        "{message}"
    );
    handle.await.unwrap();
}

#[quench_test::test]
fn debug_output_never_shows_secrets() {
    let basic = BasicAuthClient::new(DEAD, "user", "hunter2").unwrap();
    assert!(!format!("{basic:?}").contains("hunter2"));
    let builder = BasicAuthClient::builder(DEAD)
        .username("user")
        .password("hunter2");
    assert!(!format!("{builder:?}").contains("hunter2"));
    let bearer = BearerAuthClient::new(DEAD, "tok-secret").unwrap();
    assert!(!format!("{bearer:?}").contains("tok-secret"));
    let credentials = ClientCredentialsClient::builder(DEAD)
        .token_url("http://t/token")
        .client_id("svc")
        .client_secret("very-secret");
    assert!(!format!("{credentials:?}").contains("very-secret"));
    let built = credentials.build().unwrap();
    assert!(!format!("{built:?}").contains("very-secret"));
    assert!(format!("{built:?}").contains("redacted"));
}

#[quench_test::test]
fn builders_validate_their_inputs() {
    assert!(
        BasicAuthClient::builder(DEAD)
            .username("u")
            .password("p")
            .build()
            .is_ok()
    );
    assert!(
        ClientCredentialsClient::builder(DEAD)
            .token_url("http://t/token")
            .client_id("c")
            .client_secret("s")
            .build()
            .is_ok()
    );
    assert!(HttpClient::builder(DEAD).tls_verify(false).build().is_ok());
    assert!(BearerAuthClient::with_tls_verify(DEAD, "t", false).is_ok());
}
