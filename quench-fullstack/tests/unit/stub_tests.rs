//! The gatehouse-compatible stub: the shapes it answers with and the rules it keeps.

use super::support::*;
use quench_fullstack::testing::StubGatehouse;
use quench_http::prelude::Endpoint;
use quench_serde::{Value, json};

async fn call(stub: &StubGatehouse, path: &str, body: &str) -> Out {
    let container = container_with(stub.jwt()).await;
    out(stub
        .call(post(path, RPC, body.as_bytes(), &container))
        .await)
    .await
}

fn field(out: &Out, name: &str) -> String {
    let value: Value = json::from_str(&out.text()).unwrap();
    value[name].as_str().unwrap_or_default().to_string()
}

#[quench_test::test]
async fn login_answers_with_the_gatehouse_shape() {
    let stub = StubGatehouse::new(&[("ola", "pw")]);
    let out = call(
        &stub,
        "/api/v1/auth/login",
        r#"{"username":"ola","password":"pw"}"#,
    )
    .await;
    assert_eq!(out.status, 200);
    let body: Value = json::from_str(&out.text()).unwrap();
    assert_eq!(body["token_type"].as_str(), Some("Bearer"));
    assert_eq!(body["expires_in"].as_i64(), Some(900));
    assert_eq!(
        body["access_token"].as_str().unwrap().split('.').count(),
        3,
        "a JWT"
    );
    assert_eq!(body["refresh_token"].as_str().unwrap().len(), 48);
    assert_eq!(stub.login_count(), 1);
    assert!(format!("{stub:?}").contains("StubGatehouse"));
}

#[quench_test::test]
async fn login_sets_the_session_cookies_for_browsers() {
    let stub = StubGatehouse::new(&[("ola", "pw")]);
    let container = container_with(stub.jwt()).await;
    let response = stub
        .call(post(
            "/api/v1/auth/login",
            RPC,
            br#"{"username":"ola","password":"pw"}"#,
            &container,
        ))
        .await
        .into_inner();
    let cookies: Vec<&str> = response
        .headers
        .get_all("set-cookie")
        .filter_map(|v| v.to_str().ok())
        .collect();
    assert_eq!(cookies.len(), 2);
    assert!(cookies.iter().all(|c| c.contains("HttpOnly")));
}

#[quench_test::test]
async fn bad_credentials_and_bad_requests_are_refused() {
    let stub = StubGatehouse::new(&[("ola", "pw")]);
    let wrong = call(
        &stub,
        "/api/v1/auth/login",
        r#"{"username":"ola","password":"x"}"#,
    )
    .await;
    assert_eq!(wrong.status, 401);
    assert_eq!(field(&wrong, "error"), "invalid_credentials");
    assert_eq!(
        call(&stub, "/api/v1/auth/login", "not json").await.status,
        400
    );
    assert_eq!(call(&stub, "/api/v1/auth/refresh", "{}").await.status, 400);
    assert_eq!(call(&stub, "/api/v1/auth/nothing", "{}").await.status, 404);
    let container = container_with(stub.jwt()).await;
    let get = out(stub.call(request("/api/v1/auth/login", &[]).await).await).await;
    assert_eq!(get.status, 405);
    drop(container);
    assert_eq!(stub.login_count(), 0);
}

#[quench_test::test]
async fn a_refresh_token_works_once() {
    let stub = StubGatehouse::new(&[("ola", "pw")]);
    let login = call(
        &stub,
        "/api/v1/auth/login",
        r#"{"username":"ola","password":"pw"}"#,
    )
    .await;
    let first = field(&login, "refresh_token");
    let rotated = call(
        &stub,
        "/api/v1/auth/refresh",
        &format!(r#"{{"refresh_token":"{first}"}}"#),
    )
    .await;
    assert_eq!(rotated.status, 200);
    let second = field(&rotated, "refresh_token");
    assert_ne!(first, second);
    // The old one is spent: using it again is refused, as the real service does.
    let reused = call(
        &stub,
        "/api/v1/auth/refresh",
        &format!(r#"{{"refresh_token":"{first}"}}"#),
    )
    .await;
    assert_eq!(reused.status, 401);
    let again = call(
        &stub,
        "/api/v1/auth/refresh",
        &format!(r#"{{"refresh_token":"{second}"}}"#),
    )
    .await;
    assert_eq!(again.status, 200);
    assert_eq!(stub.refresh_count(), 2);
    assert_eq!(
        call(
            &stub,
            "/api/v1/auth/refresh",
            r#"{"refresh_token":"made-up"}"#
        )
        .await
        .status,
        401
    );
}

#[quench_test::test]
async fn logout_ends_the_session_and_clears_the_cookies() {
    let stub = StubGatehouse::new(&[("ola", "pw")]);
    let login = call(
        &stub,
        "/api/v1/auth/login",
        r#"{"username":"ola","password":"pw"}"#,
    )
    .await;
    let refresh = field(&login, "refresh_token");
    let container = container_with(stub.jwt()).await;
    let response = stub
        .call(post(
            "/api/v1/auth/logout",
            RPC,
            format!(r#"{{"refresh_token":"{refresh}"}}"#).as_bytes(),
            &container,
        ))
        .await
        .into_inner();
    assert_eq!(response.status.as_u16(), 204);
    let cookies: Vec<&str> = response
        .headers
        .get_all("set-cookie")
        .filter_map(|v| v.to_str().ok())
        .collect();
    assert_eq!(cookies.len(), 2);
    assert!(cookies.iter().all(|c| c.contains("Max-Age=0")));
    assert_eq!(
        call(
            &stub,
            "/api/v1/auth/refresh",
            &format!(r#"{{"refresh_token":"{refresh}"}}"#)
        )
        .await
        .status,
        401
    );
    // Logging out with nothing to log out of is fine.
    assert_eq!(
        call(&stub, "/api/v1/auth/logout", "garbage").await.status,
        204
    );
}

#[quench_test::test]
async fn revoking_everything_stops_every_refresh_token() {
    let stub = StubGatehouse::new(&[("ola", "pw"), ("ewa", "pw2")]);
    let mut tokens = Vec::new();
    for (u, p) in [("ola", "pw"), ("ewa", "pw2")] {
        let login = call(
            &stub,
            "/api/v1/auth/login",
            &format!(r#"{{"username":"{u}","password":"{p}"}}"#),
        )
        .await;
        tokens.push(field(&login, "refresh_token"));
    }
    stub.revoke_all();
    for token in tokens {
        assert_eq!(
            call(
                &stub,
                "/api/v1/auth/refresh",
                &format!(r#"{{"refresh_token":"{token}"}}"#)
            )
            .await
            .status,
            401
        );
    }
}
