//! `#[server]` functions end to end: the browser's call, the server's answer, and every shape between.

use super::functions::*;
use super::support::*;
use quench_fullstack::client;
use quench_fullstack::{FieldError, ServerError, Ui, UiApp, User};
use quench_serde::json;
use quench_ui_web::mock;

async fn app() -> (UiApp, std::sync::Arc<quench_http::di::Container>) {
    let jwt = quench_auth::domain::jwt::JwtConfig::for_tests();
    (UiApp::new(Ui::new(big())), container_with(jwt).await)
}

#[quench_test::test]
async fn a_call_runs_the_function_on_the_server_and_returns_its_value() {
    mock::reset();
    let (app, container) = app().await;
    let sum = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, i64>("/_fn/add", &json!({"a": 40, "b": 2})).await
    })
    .await;
    assert_eq!(sum, Ok(42));
    let fetch = &mock::calls();
    assert!(!fetch.is_empty());
}

#[quench_test::test]
async fn the_browser_sends_what_the_server_requires() {
    mock::reset();
    let (app, container) = app().await;
    let _ = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, i64>("/_fn/add", &json!({"a": 1, "b": 1})).await
    })
    .await;
    // The request the client made, as the mock host saw it, before the server answered.
    mock::reset();
    quench_ui_web::spawn(async {
        let _ = client::call::<_, i64>("/_fn/add", &json!({"a": 1, "b": 2})).await;
    });
    mock::run_ticks();
    let fetch = mock::fetches().remove(0);
    assert_eq!(
        (fetch.method.as_str(), fetch.url.as_str()),
        ("POST", "/_fn/add")
    );
    assert!(fetch.headers.contains("content-type: application/json"));
    assert!(fetch.headers.contains("x-quench-rpc: 1"));
    assert!(fetch.headers.contains("accept: application/json"));
    assert!(
        !fetch.headers.contains("authorization"),
        "cookie mode adds none"
    );
    assert_eq!(String::from_utf8(fetch.body).unwrap(), r#"{"a":1,"b":2}"#);
}

#[quench_test::test]
async fn a_function_with_no_arguments_takes_an_empty_object_and_returns_null() {
    mock::reset();
    let (app, container) = app().await;
    let out = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, ()>("/_fn/nothing", &json!({})).await
    })
    .await;
    assert_eq!(out, Ok(()));
}

#[quench_test::test]
async fn awkward_values_survive_the_round_trip() {
    mock::reset();
    let (app, container) = app().await;
    let note = awkward();
    let args = json!({ "note": quench_serde::to_value(&note) });
    let back = drive(&app, &container, SAME_ORIGIN, async move {
        client::call::<_, Note>("/_fn/echo", &args).await
    })
    .await;
    assert_eq!(back, Ok(note));
}

#[quench_test::test]
async fn every_enum_shape_and_option_travels() {
    mock::reset();
    let (app, container) = app().await;
    let list = vec![
        State::Open,
        State::Done {
            by: "x".into(),
            at: -5,
        },
        State::Archived(7),
        State::Pair(-128, "p".into()),
    ];
    let args = json!({ "list": quench_serde::to_value(&list) });
    let back = drive(&app, &container, SAME_ORIGIN, async move {
        client::call::<_, Vec<State>>("/_fn/states", &args).await
    })
    .await;
    assert_eq!(back, Ok(list));

    let some = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, Option<String>>("/_fn/maybe", &json!({"value": "caf\u{e9}"})).await
    })
    .await;
    assert_eq!(some, Ok(Some("CAF\u{c9}".to_string())));
    let none = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, Option<String>>("/_fn/maybe", &json!({"value": null})).await
    })
    .await;
    assert_eq!(none, Ok(None));
}

#[quench_test::test]
async fn a_function_can_refuse_with_a_validation_error_naming_the_field() {
    mock::reset();
    let (app, container) = app().await;
    let out = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, i64>("/_fn/add", &json!({"a": i64::MAX, "b": 1})).await
    })
    .await;
    assert_eq!(out, Err(ServerError::invalid("a", "too large")));
    let err = out.unwrap_err();
    assert_eq!(
        err.fields(),
        [FieldError {
            field: "a".into(),
            message: "too large".into()
        }]
    );
    assert_eq!(err.status(), 422);
}

#[quench_test::test]
async fn each_error_kind_comes_back_as_the_same_kind() {
    mock::reset();
    let (app, container) = app().await;
    for (kind, expected_status) in [
        ("unauthorized", 401),
        ("forbidden", 403),
        ("not_found", 404),
        ("validation", 422),
        ("internal", 500),
    ] {
        let out = drive(&app, &container, SAME_ORIGIN, async move {
            client::call::<_, ()>("/api/fail", &json!({"kind": kind})).await
        })
        .await;
        let err = out.unwrap_err();
        assert_eq!(err.kind(), kind, "{kind}");
        assert_eq!(err.status(), expected_status);
    }
    let ok = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, ()>("/api/fail", &json!({"kind": "none"})).await
    })
    .await;
    assert_eq!(ok, Ok(()));
}

#[quench_test::test]
async fn validation_errors_keep_their_fields() {
    mock::reset();
    let (app, container) = app().await;
    let err = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, ()>("/api/fail", &json!({"kind": "validation"})).await
    })
    .await
    .unwrap_err();
    assert_eq!(err.message(), "two things are wrong");
    assert_eq!(err.fields().len(), 2);
    assert_eq!(err.fields()[1].field, "due");
}

#[quench_test::test]
async fn an_internal_error_does_not_leak_its_cause() {
    mock::reset();
    let (app, container) = app().await;
    let err = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, ()>("/api/fail", &json!({"kind": "internal"})).await
    })
    .await
    .unwrap_err();
    assert_eq!(
        err,
        ServerError::Internal("internal server error".to_string())
    );
    assert!(!err.to_string().contains("hunter2") && !err.to_string().contains("db-7"));
}

#[quench_test::test]
async fn arguments_the_function_cannot_read_are_a_validation_error_not_a_crash() {
    mock::reset();
    let (app, container) = app().await;
    for args in [
        json!({"a": "not a number", "b": 1}),
        json!({"a": 1}),                         // b missing
        json!({"a": 1.5, "b": 1}),               // not an integer
        json!({"a": 1, "b": 2, "c": 3}).clone(), // extra fields are tolerated
    ] {
        let out = drive(&app, &container, SAME_ORIGIN, async move {
            client::call::<_, i64>("/_fn/add", &args).await
        })
        .await;
        match out {
            Ok(n) => assert_eq!(n, 3, "only the call with an extra field may succeed"),
            Err(e) => assert_eq!(e.kind(), "validation", "{e}"),
        }
    }
    // Not even JSON.
    mock::reset();
    quench_ui_web::spawn(async {
        let _ = quench_ui_web::Request::post("/_fn/add")
            .header("content-type", "application/json")
            .header("x-quench-rpc", "1")
            .body("{not json")
            .send()
            .await;
    });
    mock::run_ticks();
    assert_eq!(pump(&app, &container, SAME_ORIGIN).await, 1);
}

#[quench_test::test]
async fn an_auth_function_sees_the_user_through_user() {
    mock::reset();
    let (app, container) = app().await;
    // With no session configured (auth off) the context is the development administrator.
    let jwt = quench_auth::domain::jwt::JwtConfig::for_tests();
    assert!(!jwt.auth_enabled);
    let user = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, User>("/_fn/who", &json!({})).await
    })
    .await
    .unwrap();
    assert_eq!(user.sub, "admin");
    let shout = drive(&app, &container, SAME_ORIGIN, async {
        client::call::<_, String>("/_fn/shout", &json!({"text": "hi"})).await
    })
    .await;
    assert_eq!(shout, Ok("admin: HI".to_string()));
}

#[quench_test::test]
async fn the_functions_are_also_ordinary_functions_on_the_server() {
    assert_eq!(add(2, 3).await, Ok(5));
    let user = User {
        sub: "ola".into(),
        scope: "user notes:write".into(),
        audiences: vec![],
        session: None,
    };
    let shouted = quench_fullstack::server::with_user(Some(user.clone()), shout("x".into())).await;
    assert_eq!(shouted, Ok("ola: X".to_string()));
    // Outside a call there is nobody, whatever the function was marked with.
    assert_eq!(quench_fullstack::user().unwrap_err().kind(), "unauthorized");
    assert!(user.has_role("notes:write") && !user.has_role("notes") && !user.has_role("admin"));
}

#[quench_test::test]
async fn the_server_lists_every_function_it_linked() {
    let paths: Vec<&str> = quench_fullstack::server::functions()
        .iter()
        .map(|(p, _)| *p)
        .collect();
    for expected in [
        "/_fn/add",
        "/_fn/echo",
        "/_fn/states",
        "/_fn/nothing",
        "/_fn/maybe",
        "/_fn/who",
        "/_fn/shout",
        "/_fn/maybe_who",
        "/_fn/forgot_auth",
        "/api/fail",
    ] {
        assert!(paths.contains(&expected), "{expected} in {paths:?}");
    }
    assert!(format!("{:?}", UiApp::new(Ui::new(big()))).contains("/_fn/add"));
}

#[quench_test::test]
fn two_functions_with_one_path_are_refused_at_startup() {
    use quench_fullstack::server::{ServerFn, collect};
    use std::sync::Arc;
    fn endpoint() -> Arc<dyn quench_http::prelude::Endpoint> {
        Arc::new(quench_fullstack::testing::StubGatehouse::new(&[]))
    }
    static A: ServerFn = ServerFn {
        path: "/_fn/same",
        endpoint,
    };
    static B: ServerFn = ServerFn {
        path: "/_fn/same",
        endpoint,
    };
    static C: ServerFn = ServerFn {
        path: "/_fn/other",
        endpoint,
    };
    assert_eq!(collect([&A, &C]).len(), 2);
    let result = std::panic::catch_unwind(|| collect([&A, &B]));
    let Err(panic) = result else {
        panic!("a duplicate path must be refused")
    };
    let message = *panic.downcast::<String>().unwrap();
    assert!(
        message.contains("/_fn/same") && message.contains("share the path"),
        "{message}"
    );
}
