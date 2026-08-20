//! Unit tests for `actix/domain/auth.rs`.

use quench_auth::actix::domain::auth::*;

fn grants(pairs: &[(&str, &[&str])]) -> Permissions {
    pairs
        .iter()
        .map(|(service, actions)| {
            (
                (*service).to_string(),
                actions.iter().map(|action| action.to_string()).collect(),
            )
        })
        .collect()
}

fn user(roles: Vec<Role>, permissions: &[(&str, &[&str])]) -> User {
    User::new(
        "someone".into(),
        "password".into(),
        roles,
        grants(permissions),
        None,
    )
    .unwrap()
}

#[test]
fn passwords_use_argon2id_with_unique_salts() {
    let first = User::new(
        "first".into(),
        "password".into(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();
    let second = User::new(
        "second".into(),
        "password".into(),
        vec![Role::User],
        Permissions::new(),
        None,
    )
    .unwrap();

    assert!(first.password.starts_with("$argon2id$"));
    assert_ne!(first.password, second.password);
    assert!(first.verify_password("password"));
    assert!(!first.verify_password("wrong"));
}

#[test]
fn an_ordinary_user_holds_only_what_is_granted() {
    let user = user(vec![Role::User], &[("sage", &["read"])]);

    assert!(user.can("sage", "read"));
    assert!(!user.can("sage", "write"));
    assert!(!user.can("warehouse", "read"));
}

/// The whole point of a flat action set rather than an ordered level: two
/// unrelated actions on the same service are independently grantable, and
/// holding one says nothing about the other.
#[test]
fn actions_on_one_service_are_independent() {
    let user = user(vec![Role::User], &[("switchboard", &["launch"])]);

    assert!(user.can("switchboard", "launch"));
    assert!(!user.can("switchboard", "stop"));
    assert!(!user.can("switchboard", "delete-model"));
}

/// The wildcard is the whole reason adding a service to the estate does not mean
/// re-granting anything to administrators.
#[test]
fn wildcard_roles_reach_a_service_with_no_grant_written_down() {
    for role in [Role::Admin, Role::Service] {
        let holder = user(vec![role.clone()], &[]);

        assert!(holder.has_wildcard(), "{role} should be a wildcard");
        assert!(holder.can("a-service-invented-just-now", "anything-at-all"));
        assert!(
            holder.get_permissions().is_empty(),
            "a wildcard should not need enumerated grants"
        );
    }
}

#[test]
fn the_plain_user_role_is_not_a_wildcard() {
    assert!(!Role::User.is_wildcard());
    assert!(!user(vec![Role::User], &[]).can("sage", "read"));
}

/// A row that will not parse as `{service: [action, ...]}` reads as no grants
/// at all, not a partial one - "no access" is the safe direction for that
/// failure to go.
#[test]
fn an_unreadable_permissions_column_reads_as_no_grants() {
    let mut user = user(vec![Role::User], &[("sage", &["read"])]);
    user.permissions = serde_json::json!("not an object");

    assert!(user.get_permissions().is_empty());
    assert!(!user.can("sage", "read"));
}

/// Omit this from `columns` and CRUD silently drops the column on every write.
#[test]
fn permissions_is_a_persisted_column() {
    use quench_db::prelude::Model;
    assert!(User::columns().contains(&"permissions"));
}

#[test]
fn a_fresh_user_is_neither_disabled_nor_locked() {
    let user = user(vec![Role::User], &[]);
    assert!(!user.is_disabled());
    assert!(!user.is_locked());
}

#[test]
fn failed_logins_below_the_threshold_do_not_lock() {
    let mut u = user(vec![Role::User], &[]);
    for _ in 0..4 {
        u.record_failed_login(5, chrono::Duration::minutes(15));
    }
    assert_eq!(u.failed_login_attempts, 4);
    assert!(!u.is_locked());
}

#[test]
fn the_nth_failed_login_locks_and_resets_the_counter() {
    let mut u = user(vec![Role::User], &[]);
    for _ in 0..5 {
        u.record_failed_login(5, chrono::Duration::minutes(15));
    }
    assert!(u.is_locked());
    // The count is "failures since the last lock", not a running total - it
    // must not still read 5 once a lock has just been applied.
    assert_eq!(u.failed_login_attempts, 0);
}

#[test]
fn a_lock_expires_on_its_own_once_the_duration_passes() {
    let mut u = user(vec![Role::User], &[]);
    u.locked_until = Some(chrono::Utc::now() - chrono::Duration::seconds(1));
    assert!(!u.is_locked());
}

#[test]
fn a_successful_login_clears_the_failure_count_and_stamps_last_login() {
    let mut u = user(vec![Role::User], &[]);
    u.record_failed_login(5, chrono::Duration::minutes(15));
    u.record_failed_login(5, chrono::Duration::minutes(15));
    assert!(u.last_login_at.is_none());

    u.record_successful_login();

    assert_eq!(u.failed_login_attempts, 0);
    assert!(u.last_login_at.is_some());
}

#[test]
fn disabled_is_driven_by_disabled_at_alone() {
    let mut u = user(vec![Role::User], &[]);
    assert!(!u.is_disabled());
    u.disabled_at = Some(chrono::Utc::now());
    assert!(u.is_disabled());
}

#[test]
fn role_parse_round_trips_through_display_and_rejects_unknown_values() {
    for role in [Role::Admin, Role::User, Role::Service] {
        assert_eq!(Role::parse(&role.to_string()), Some(role));
    }
    assert_eq!(Role::parse("  ADMIN  "), Some(Role::Admin));
    assert_eq!(Role::parse("nonsense"), None);
}
