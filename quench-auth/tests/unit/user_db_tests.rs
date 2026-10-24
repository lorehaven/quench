//! `UserDb` over the in-memory database, plus the `Model` shape of `User`.

use quench_auth::domain::auth::{Permissions, Role, User, UserDb};
use quench_core::time::{Duration, Time};
use quench_db::{Crud, Db, Model};

async fn setup() -> (Db, std::sync::Arc<UserDb>) {
    let db = Db::connect("").await.unwrap();
    let users = UserDb::init(db.clone()).await;
    (db, users)
}

fn alice() -> User {
    User::new(
        "alice".into(),
        "correct horse".into(),
        vec![Role::User],
        Permissions::new(),
        Some("alice@example.com".into()),
    )
    .unwrap()
}

#[quench_test::test]
async fn users_are_found_by_name() {
    let (db, users) = setup().await;
    db.repository::<User>().create(&alice()).await.unwrap();
    let found = users.get_user("alice").await.expect("present");
    assert_eq!(found.email.as_deref(), Some("alice@example.com"));
    assert!(users.get_user("nobody").await.is_none());
    assert!(format!("{users:?}").contains("Base"));
}

#[quench_test::test]
async fn validation_checks_the_password() {
    let (db, users) = setup().await;
    db.repository::<User>().create(&alice()).await.unwrap();
    assert!(users.validate("alice", "correct horse").await.is_some());
    assert!(users.validate("alice", "wrong").await.is_none());
    assert!(users.validate("nobody", "x").await.is_none());
}

#[quench_test::test]
async fn disabled_and_locked_accounts_cannot_authenticate() {
    let (db, users) = setup().await;
    let repo = db.repository::<User>();

    let mut disabled = alice();
    disabled.username = "disabled".into();
    disabled.disabled_at = Some(Time::now());
    repo.create(&disabled).await.unwrap();
    assert!(users.validate("disabled", "correct horse").await.is_none());

    let mut locked = alice();
    locked.username = "locked".into();
    locked.locked_until = Some(Time::now() + Duration::minutes(5));
    repo.create(&locked).await.unwrap();
    assert!(users.validate("locked", "correct horse").await.is_none());

    let mut expired = alice();
    expired.username = "expired-lock".into();
    expired.locked_until = Some(Time::now() - Duration::minutes(5));
    repo.create(&expired).await.unwrap();
    assert!(
        users
            .validate("expired-lock", "correct horse")
            .await
            .is_some(),
        "a lock that ran out no longer blocks"
    );
}

#[quench_test::test]
fn the_model_maps_to_the_users_table() {
    assert_eq!(User::table_name().rsplit('.').next(), Some("users"));
    assert_eq!(User::primary_key_name(), "username");
    let columns = User::columns();
    for expected in [
        "username",
        "password",
        "roles",
        "permissions",
        "email",
        "locked_until",
        "mfa_enabled",
        "preferred_locale",
    ] {
        assert!(columns.contains(&expected), "{expected}");
    }
    assert_eq!(columns.len(), 19);
}

#[quench_test::test]
fn users_survive_a_serialization_round_trip() {
    let user = alice();
    let back: User = quench_serde::from_value(&quench_serde::to_value(&user)).unwrap();
    assert_eq!(back.username, user.username);
    assert_eq!(back.created_at, user.created_at);
    assert_eq!(back.get_roles(), vec![Role::User]);
}
