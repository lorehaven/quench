//! Who is calling: sessions by cookie and by bearer token, expiry, refresh, and signing out.

use super::support::*;
use quench_auth::domain::jwt::JwtConfig;
use quench_fullstack::client::{self, Bearer, MemoryStore, TokenStore, Tokens};
use quench_fullstack::testing::StubGatehouse;
use quench_fullstack::{ServerError, Ui, UiApp, User};
use quench_http::di::Container;
use quench_http::prelude::{Endpoint, Request, Response};
use quench_serde::json;
use quench_ui_web::mock;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// The app and the token service behind one address, as a real deployment fronts them.
struct Both {
    app: UiApp,
    auth: StubGatehouse,
}

#[quench_core::async_trait]
impl Endpoint for Both {
    async fn call(&self, req: Request) -> Response {
        if req.uri().path().starts_with("/api/v1/auth/") {
            self.auth.call(req).await
        } else {
            self.app.call(req).await
        }
    }
}

struct World {
    both: Both,
    stub: StubGatehouse,
    container: Arc<Container>,
}

async fn world() -> World {
    mock::reset();
    client::reset();
    let stub = StubGatehouse::new(&[("ola", "pw"), ("ewa", "pw2")]);
    let mut jwt = stub.jwt();
    jwt.auth_enabled = true;
    let container = container_with(jwt).await;
    World {
        both: Both {
            app: UiApp::new(Ui::new(big())),
            auth: stub.clone(),
        },
        stub,
        container,
    }
}

fn bearer(store: &Rc<MemoryStore>) -> Bearer {
    Bearer::new("/api/v1/auth", store.clone())
}

async fn who_now(w: &World) -> Result<User, ServerError> {
    drive(&w.both, &w.container, &[], async {
        client::call::<_, User>("/_fn/who", &json!({})).await
    })
    .await
}

#[quench_test::test]
async fn a_call_with_no_session_is_unauthorised_and_tells_the_app() {
    let w = world().await;
    let told = Rc::new(Cell::new(0));
    let t = told.clone();
    client::on_unauthorized(move || t.set(t.get() + 1));
    let err = who_now(&w).await.unwrap_err();
    assert_eq!(err.kind(), "unauthorized");
    assert_eq!(err.status(), 401);
    assert_eq!(told.get(), 1, "the sign-in handler ran once");
}

#[quench_test::test]
async fn a_valid_bearer_token_is_the_user() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    let login = drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.login("ola", "pw").await }
    })
    .await;
    assert_eq!(login, Ok(()));
    assert!(session.signed_in());
    client::use_bearer(session);
    let user = who_now(&w).await.unwrap();
    assert_eq!(user.sub, "ola");
    assert_eq!(user.audiences, vec!["service".to_string()]);
    assert!(user.session.is_some());
    // Each call carries the token.
    let shout = drive(&w.both, &w.container, &[], async {
        client::call::<_, String>("/_fn/shout", &json!({"text": "hello"})).await
    })
    .await;
    assert_eq!(shout, Ok("ola: HELLO".to_string()));
    assert_eq!((w.stub.login_count(), w.stub.refresh_count()), (1, 0));
}

#[quench_test::test]
async fn the_wrong_password_is_refused_and_stores_nothing() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    for (user, password) in [("ola", "nope"), ("nobody", "pw"), ("", "")] {
        let out = drive(&w.both, &w.container, &[], {
            let session = session.clone();
            async move { session.login(user, password).await }
        })
        .await;
        assert_eq!(
            out,
            Err(ServerError::Unauthorized("invalid credentials".into())),
            "{user}"
        );
    }
    assert!(!session.signed_in() && store.load().is_none());
    assert_eq!(w.stub.login_count(), 0);
}

#[quench_test::test]
async fn an_expired_token_is_refreshed_and_the_call_retried_once() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    w.stub.set_access_ttl(-120); // issued already expired (past the 60-second leeway)
    drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.login("ola", "pw").await }
    })
    .await
    .unwrap();
    let expired = store.load().unwrap();
    w.stub.set_access_ttl(900);
    client::use_bearer(session);
    let user = who_now(&w).await.unwrap();
    assert_eq!(user.sub, "ola");
    assert_eq!(w.stub.refresh_count(), 1, "one refresh");
    let now = store.load().unwrap();
    assert_ne!(now.access, expired.access, "a new access token");
    assert_ne!(
        now.refresh, expired.refresh,
        "and the refresh token rotated"
    );
    // Later calls use the new token and need no refresh.
    who_now(&w).await.unwrap();
    assert_eq!(w.stub.refresh_count(), 1);
}

#[quench_test::test]
async fn several_calls_that_meet_an_expired_token_share_one_refresh() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    w.stub.set_access_ttl(-120);
    drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.login("ola", "pw").await }
    })
    .await
    .unwrap();
    w.stub.set_access_ttl(900);
    client::use_bearer(session);

    let results: Rc<RefCell<Vec<Result<User, ServerError>>>> = Rc::default();
    for _ in 0..5 {
        let results = results.clone();
        quench_ui_web::spawn(async move {
            let r = client::call::<_, User>("/_fn/who", &json!({})).await;
            results.borrow_mut().push(r);
        });
    }
    for _ in 0..200 {
        mock::run_ticks();
        if results.borrow().len() == 5 {
            break;
        }
        if pump(&w.both, &w.container, &[]).await == 0
            && let Some((id, _)) = mock::timers().first().copied()
        {
            mock::fire_timer(id);
        }
    }
    let results = results.borrow();
    assert_eq!(results.len(), 5);
    assert!(
        results
            .iter()
            .all(|r| r.as_ref().is_ok_and(|u| u.sub == "ola")),
        "{results:?}"
    );
    assert_eq!(
        w.stub.refresh_count(),
        1,
        "the rotated refresh token was used once, not five times"
    );
}

#[quench_test::test]
async fn when_the_refresh_token_is_dead_the_session_ends() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    w.stub.set_access_ttl(-120);
    drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.login("ola", "pw").await }
    })
    .await
    .unwrap();
    w.stub.revoke_all();
    let told = Rc::new(Cell::new(0));
    let t = told.clone();
    client::on_unauthorized(move || t.set(t.get() + 1));
    client::use_bearer(session.clone());
    let err = who_now(&w).await.unwrap_err();
    assert_eq!(err.kind(), "unauthorized");
    assert!(!session.signed_in(), "the dead tokens are forgotten");
    assert_eq!(told.get(), 1);
    assert_eq!(w.stub.refresh_count(), 0, "the server refused the refresh");
}

#[quench_test::test]
async fn refresh_without_tokens_and_logout_are_safe() {
    let w = world().await;
    let store = Rc::new(MemoryStore::default());
    let session = bearer(&store);
    let out = drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.refresh().await }
    })
    .await;
    assert_eq!(out, Err(ServerError::Unauthorized("not signed in".into())));

    drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.login("ewa", "pw2").await }
    })
    .await
    .unwrap();
    let refresh = store.load().unwrap().refresh;
    drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.logout().await }
    })
    .await;
    assert!(!session.signed_in());
    // The server forgot the session too: the old refresh token is worthless.
    store.save(Some(Tokens {
        access: "x".into(),
        refresh,
    }));
    let out = drive(&w.both, &w.container, &[], {
        let session = session.clone();
        async move { session.refresh().await }
    })
    .await;
    assert_eq!(out.unwrap_err().kind(), "unauthorized");
    assert!(format!("{session:?}").contains("Bearer"));
}

#[quench_test::test]
async fn a_cookie_session_is_the_user_without_any_token_in_the_page() {
    let w = world().await;
    // The browser signs in; the answer sets the cookies and the page never sees a token.
    let out = drive(&w.both, &w.container, &[], async {
        client::sign_in("/api/v1/auth", "ola", "pw").await
    })
    .await;
    assert_eq!(out, Ok(()));
    // Reproduce what the browser does next: send the cookie the stub set.
    let login = post(
        "/api/v1/auth/login",
        RPC,
        br#"{"username":"ola","password":"pw"}"#,
        &w.container,
    );
    let response = w.both.call(login).await.into_inner();
    let cookies: Vec<String> = response
        .headers
        .get_all("set-cookie")
        .filter_map(|v| v.to_str().ok())
        .map(str::to_string)
        .collect();
    assert_eq!(cookies.len(), 2, "session and refresh cookies: {cookies:?}");
    let session_cookie = cookies
        .iter()
        .find(|c| c.starts_with(&quench_auth::domain::realm::session_cookie_name()))
        .unwrap();
    let pair = session_cookie.split(';').next().unwrap();
    assert!(
        cookies.iter().all(|c| c.contains("HttpOnly")),
        "script cannot read them: {cookies:?}"
    );
    let headers = with(RPC, SAME_ORIGIN);
    let mut with_cookie: Vec<(String, String)> = headers;
    with_cookie.push(("cookie".into(), pair.to_string()));
    let answer = out_of(&w, "/_fn/who", &with_cookie).await;
    assert_eq!(answer.status, 200, "{}", answer.text());
    assert!(answer.text().contains("\"sub\":\"ola\""));
}

async fn out_of(w: &World, path: &str, headers: &[(String, String)]) -> Out {
    out(w.both.call(post(path, headers, b"{}", &w.container)).await).await
}

#[quench_test::test]
async fn bad_tokens_are_not_a_session() {
    let w = world().await;
    let good = w
        .stub
        .jwt()
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    let header = |token: &str| with(RPC, &[("authorization", &format!("Bearer {token}"))]);

    let ok = out_of(&w, "/_fn/who", &header(&good)).await;
    assert_eq!(ok.status, 200, "{}", ok.text());

    // Tampered: a changed character in the signature.
    let mut tampered = good.clone();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });
    assert_eq!(out_of(&w, "/_fn/who", &header(&tampered)).await.status, 401);

    // Not a token at all, and an empty one.
    for junk in ["", "abc", "a.b.c", "Bearer"] {
        assert_eq!(
            out_of(&w, "/_fn/who", &header(junk)).await.status,
            401,
            "{junk:?}"
        );
    }
    // Expired.
    w.stub.set_access_ttl(-120);
    let mut stale = w.stub.jwt();
    stale.access_token_ttl_secs = -120;
    let expired = stale
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    assert_eq!(out_of(&w, "/_fn/who", &header(&expired)).await.status, 401);

    // Signed by someone else.
    let other = JwtConfig::for_tests_with_signing()
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    assert_eq!(out_of(&w, "/_fn/who", &header(&other)).await.status, 401);
}

#[quench_test::test]
async fn a_token_for_another_service_is_not_accepted() {
    let w = world().await;
    let mut other_audience = w.stub.jwt();
    other_audience.service_name = "somewhere-else".to_string();
    other_audience.audiences = vec!["somewhere-else".to_string()];
    let token = other_audience
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    let headers = with(RPC, &[("authorization", &format!("Bearer {token}"))]);
    assert_eq!(out_of(&w, "/_fn/who", &headers).await.status, 401);
}

#[quench_test::test]
async fn a_service_without_authentication_configured_says_so_instead_of_letting_everyone_in() {
    mock::reset();
    let container = Arc::new(
        quench_http::prelude::ContainerBuilder::new()
            .build()
            .await
            .unwrap(),
    );
    let app = UiApp::new(Ui::new(big()));
    let out = out(app
        .call(post("/_fn/who", &with(RPC, SAME_ORIGIN), b"{}", &container))
        .await)
    .await;
    assert_eq!(out.status, 500);
    assert!(
        !out.text().contains("JwtConfig"),
        "the details stay in the log: {}",
        out.text()
    );
}

#[quench_test::test]
async fn an_auth_function_is_not_run_without_a_session_and_an_unmarked_one_has_no_user() {
    let w = world().await;
    // `shout` is `auth`: refused before it runs.
    let refused = out_of(&w, "/_fn/shout", &with(RPC, SAME_ORIGIN)).await;
    assert_eq!(refused.status, 401);
    // `forgot_auth` was not marked, so even a signed-in caller has no `user()` in it.
    let token = w
        .stub
        .jwt()
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    let signed_in = with(RPC, &[("authorization", &format!("Bearer {token}"))]);
    let out = out_of(&w, "/_fn/forgot_auth", &signed_in).await;
    assert_eq!(out.status, 401, "{}", out.text());
}

#[quench_test::test]
async fn optional_auth_runs_for_everyone_and_knows_who_it_can() {
    let w = world().await;
    let anonymous = out_of(&w, "/_fn/maybe_who", &with(RPC, SAME_ORIGIN)).await;
    assert_eq!((anonymous.status, anonymous.text().as_str()), (200, "null"));
    let token = w
        .stub
        .jwt()
        .issue_access_token("ewa".into(), "user".into(), None)
        .await
        .unwrap();
    let signed_in = with(RPC, &[("authorization", &format!("Bearer {token}"))]);
    let known = out_of(&w, "/_fn/maybe_who", &signed_in).await;
    assert_eq!((known.status, known.text().as_str()), (200, "\"ewa\""));
    // A bad token is just no user, not an error.
    let junk = with(RPC, &[("authorization", "Bearer nonsense")]);
    assert_eq!(out_of(&w, "/_fn/maybe_who", &junk).await.text(), "null");
}

#[quench_test::test]
async fn a_page_can_depend_on_who_is_asking() {
    let w = world().await;
    let app = UiApp::new(Ui::new(big())).page_for_user("/me", |_req, user| {
        quench_fullstack::Page::new(
            "me",
            user.map_or_else(|| "nobody".to_string(), |u| format!("hello {}", u.sub)),
        )
    });
    let anonymous = out(app
        .call(post("/me", &with(RPC, SAME_ORIGIN), b"", &w.container))
        .await)
    .await;
    assert!(anonymous.text().contains("nobody"), "{}", anonymous.text());
    let token = w
        .stub
        .jwt()
        .issue_access_token("ola".into(), "user".into(), None)
        .await
        .unwrap();
    let headers = with(RPC, &[("authorization", &format!("Bearer {token}"))]);
    let known = out(app.call(post("/me", &headers, b"", &w.container)).await).await;
    assert!(known.text().contains("hello ola"), "{}", known.text());
}
