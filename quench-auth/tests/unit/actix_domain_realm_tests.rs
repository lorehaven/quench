//! Unit tests for `actix/domain/realm.rs`.

use quench_auth::actix::domain::realm::*;

/// `envmnt` is process-global, so these assertions share one test to keep
/// them from racing each other - and `GATEHOUSE_URL` is also touched by
/// jwks/sso_client tests elsewhere in this binary, hence the shared lock.
#[test]
fn realm_names_come_from_the_environment() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("AUTH_DB_SCHEMA");
    envmnt::remove("AUTH_COOKIE_NAME");
    envmnt::remove("GATEHOUSE_URL");

    assert_eq!(auth_schema(), "auth");
    assert_eq!(session_cookie_name(), "forge_session");
    assert_eq!(refresh_cookie_name(), "forge_refresh");
    assert!(gatehouse_login_url(None).is_none());
    assert!(gatehouse_logout_url(None).is_none());

    envmnt::set("GATEHOUSE_URL", "https://gate.example.com/gatehouse/");
    assert_eq!(
        gatehouse_login_url(None).unwrap(),
        "https://gate.example.com/gatehouse/ui/login"
    );
    assert_eq!(
        gatehouse_login_url(Some("https://sage.example.com/ui/home")).unwrap(),
        "https://gate.example.com/gatehouse/ui/login?redirect=https%3A%2F%2Fsage.example.com%2Fui%2Fhome"
    );
    assert_eq!(
        gatehouse_logout_url(None).unwrap(),
        "https://gate.example.com/gatehouse/ui/logout"
    );
    // An empty return-to is treated the same as none - no `?redirect=` at all.
    assert_eq!(
        gatehouse_logout_url(Some("")).unwrap(),
        "https://gate.example.com/gatehouse/ui/logout"
    );
    envmnt::remove("GATEHOUSE_URL");
}

/// `AUTH_COOKIE_DOMAIN` is also touched by `cookie_domain_is_applied_...`
/// below, hence the shared lock.
#[test]
fn session_and_refresh_cookies_carry_the_expected_attributes() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::remove("AUTH_COOKIE_NAME");
    envmnt::remove("AUTH_REFRESH_COOKIE_NAME");
    envmnt::remove("AUTH_COOKIE_DOMAIN");
    envmnt::remove("REFRESH_TOKEN_TTL_SECS");

    let session = session_cookie("tok");
    assert_eq!(session.name(), "forge_session");
    assert_eq!(session.value(), "tok");
    assert_eq!(session.http_only(), Some(true));
    assert_eq!(session.secure(), Some(true));
    assert_eq!(session.same_site(), Some(actix_web::cookie::SameSite::Lax));
    assert_eq!(session.path(), Some("/"));
    // A session cookie has no explicit `Max-Age` - it dies with the browser
    // session, unlike the refresh cookie below.
    assert!(session.max_age().is_none());

    let refresh = refresh_cookie("tok2");
    assert_eq!(refresh.name(), "forge_refresh");
    assert_eq!(refresh.value(), "tok2");
    assert_eq!(
        refresh.max_age(),
        Some(actix_web::cookie::time::Duration::seconds(604_800))
    );

    envmnt::set("REFRESH_TOKEN_TTL_SECS", "3600");
    let refresh = refresh_cookie("tok3");
    assert_eq!(
        refresh.max_age(),
        Some(actix_web::cookie::time::Duration::seconds(3600))
    );
    envmnt::remove("REFRESH_TOKEN_TTL_SECS");
}

#[test]
fn cleared_cookies_are_empty_and_expire_immediately() {
    envmnt::remove("AUTH_COOKIE_NAME");
    envmnt::remove("AUTH_REFRESH_COOKIE_NAME");

    let cleared_session = cleared_session_cookie();
    assert_eq!(cleared_session.name(), "forge_session");
    assert_eq!(cleared_session.value(), "");
    assert_eq!(
        cleared_session.max_age(),
        Some(actix_web::cookie::time::Duration::seconds(0))
    );

    let cleared_refresh = cleared_refresh_cookie();
    assert_eq!(cleared_refresh.name(), "forge_refresh");
    assert_eq!(
        cleared_refresh.max_age(),
        Some(actix_web::cookie::time::Duration::seconds(0))
    );
}

/// Shares `AUTH_COOKIE_DOMAIN` with `session_and_refresh_cookies_...` above.
#[test]
fn cookie_domain_is_applied_to_realm_cookies_when_set() {
    let _guard = crate::env_lock::ENV_LOCK.lock().unwrap();
    envmnt::set("AUTH_COOKIE_DOMAIN", ".example.com");
    assert_eq!(cookie_domain().as_deref(), Some(".example.com"));

    let session = session_cookie("tok");
    assert_eq!(session.domain(), Some(".example.com"));
    envmnt::remove("AUTH_COOKIE_DOMAIN");

    assert!(cookie_domain().is_none());
    let session = session_cookie("tok");
    assert!(session.domain().is_none());
}

#[test]
fn authorize_state_cookie_and_its_cleared_counterpart() {
    let cookie = authorize_state_cookie("state-blob");
    assert_eq!(cookie.name(), AUTHORIZE_STATE_COOKIE);
    assert_eq!(cookie.value(), "state-blob");
    assert_eq!(cookie.http_only(), Some(true));
    assert_eq!(
        cookie.max_age(),
        Some(actix_web::cookie::time::Duration::minutes(5))
    );

    let cleared = cleared_authorize_state_cookie();
    assert_eq!(cleared.name(), AUTHORIZE_STATE_COOKIE);
    assert_eq!(cleared.value(), "");
    assert_eq!(
        cleared.max_age(),
        Some(actix_web::cookie::time::Duration::seconds(0))
    );
}

/// `BASE_PATH` is shared by `base_path` and `ui_path`, so both live in one
/// test to keep them from racing each other.
#[test]
fn base_path_normalizes_every_shape_of_raw_value_and_ui_path_builds_on_it() {
    envmnt::set("BASE_PATH", "");
    assert_eq!(base_path(), "/");
    assert_eq!(ui_path("/home"), "/ui/home");

    envmnt::set("BASE_PATH", "/");
    assert_eq!(base_path(), "/");

    envmnt::set("BASE_PATH", "   ");
    assert_eq!(base_path(), "/");

    envmnt::set("BASE_PATH", "/api");
    assert_eq!(base_path(), "/api");
    assert_eq!(ui_path("/home"), "/api/ui/home");

    envmnt::set("BASE_PATH", "/api/");
    assert_eq!(base_path(), "/api");

    // No leading slash gets one added.
    envmnt::set("BASE_PATH", "api");
    assert_eq!(base_path(), "/api");

    envmnt::remove("BASE_PATH");
}
