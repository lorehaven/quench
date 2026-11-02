//! Unit tests for `domain/session.rs`.

use quench_auth::domain::session::*;
use quench_cache::CacheStore;
use std::sync::Arc;

fn sessions() -> Arc<SessionDb> {
    SessionDb::init(CacheStore::in_memory())
}

#[quench_test::test]
async fn refresh_tokens_rotate_and_the_new_one_works() {
    let sessions = sessions();
    let (session, first) = sessions.create("user", 3600).await.unwrap();

    let (_, second) = sessions.rotate(&first, 3600).await.unwrap().unwrap();

    assert!(sessions.rotate(&second, 3600).await.unwrap().is_some());
    assert!(sessions.is_active(&session.id, "user").await.unwrap());
}

/// The fix for a client that rotated its token and then never found out - see
/// this module's own doc comment. A second presentation of the same
/// now-consumed token has to succeed once, with the exact token the first
/// rotation already produced, or a crash between gatehouse minting that token
/// and the client persisting it would lock the client out for good.
#[quench_test::test]
async fn a_just_rotated_token_still_works_once_more_for_a_lost_response() {
    let sessions = sessions();
    let (_, first) = sessions.create("user", 3600).await.unwrap();

    let (_, second) = sessions.rotate(&first, 3600).await.unwrap().unwrap();
    let (_, replayed) = sessions.rotate(&first, 3600).await.unwrap().unwrap();

    assert_eq!(
        replayed, second,
        "the recovered token has to be the one already-completed rotation, not a new one"
    );
}

/// The recovery in the test above is itself single-use: it exists to answer
/// the caller's very next retry, not to keep a captured old token replayable
/// for the whole grace window.
#[quench_test::test]
async fn a_recovered_token_does_not_work_a_second_time() {
    let sessions = sessions();
    let (_, first) = sessions.create("user", 3600).await.unwrap();

    sessions.rotate(&first, 3600).await.unwrap().unwrap();
    sessions.rotate(&first, 3600).await.unwrap().unwrap();

    assert!(
        sessions.rotate(&first, 3600).await.unwrap().is_none(),
        "a third presentation of an old token must not work"
    );
}

#[quench_test::test]
async fn revoking_ends_the_session_for_its_owner_only() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(!sessions.revoke(&session.id, "bob").await.unwrap());
    assert!(sessions.is_active(&session.id, "alice").await.unwrap());

    assert!(sessions.revoke(&session.id, "alice").await.unwrap());
    assert!(!sessions.is_active(&session.id, "alice").await.unwrap());
    assert!(
        sessions.rotate(&token, 3600).await.unwrap().is_none(),
        "a revoked session's refresh token is dead too"
    );
}

#[quench_test::test]
async fn a_session_belongs_to_one_user() {
    let sessions = sessions();
    let (session, _) = sessions.create("alice", 3600).await.unwrap();
    assert!(!sessions.is_active(&session.id, "bob").await.unwrap());
}

#[quench_test::test]
async fn logout_revokes_by_refresh_token() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(sessions.revoke_by_refresh_token(&token).await.unwrap());
    assert!(!sessions.is_active(&session.id, "alice").await.unwrap());
    assert!(!sessions.revoke_by_refresh_token(&token).await.unwrap());
}

/// What makes removing someone's permissions take effect now rather than
/// whenever their access token happens to expire.
#[quench_test::test]
async fn revoking_everything_ends_every_session_one_user_holds() {
    let sessions = sessions();
    let (first, first_token) = sessions.create("alice", 3600).await.unwrap();
    let (second, _) = sessions.create("alice", 3600).await.unwrap();
    let (bystander, _) = sessions.create("bob", 3600).await.unwrap();

    assert_eq!(sessions.revoke_all("alice").await.unwrap(), 2);

    assert!(!sessions.is_active(&first.id, "alice").await.unwrap());
    assert!(!sessions.is_active(&second.id, "alice").await.unwrap());
    assert!(
        sessions.rotate(&first_token, 3600).await.unwrap().is_none(),
        "the refresh tokens have to die with the sessions"
    );
    assert!(
        sessions.is_active(&bystander.id, "bob").await.unwrap(),
        "one user's revocation must not touch another's"
    );

    assert_eq!(sessions.revoke_all("alice").await.unwrap(), 0);
    assert!(sessions.sessions_for("alice").await.unwrap().is_empty());
}

/// The index is what `revoke_all` walks. A session missing from it would survive
/// a revocation, so every path that creates or extends one has to maintain it.
#[quench_test::test]
async fn the_index_tracks_sessions_through_creation_rotation_and_logout() {
    let sessions = sessions();
    let (session, token) = sessions.create("alice", 3600).await.unwrap();

    assert_eq!(
        sessions.sessions_for("alice").await.unwrap(),
        vec![session.id.clone()]
    );

    // Rotation keeps the same session id, so the index should not grow.
    let (rotated, _) = sessions.rotate(&token, 3600).await.unwrap().unwrap();
    assert_eq!(rotated.id, session.id);
    assert_eq!(
        sessions.sessions_for("alice").await.unwrap(),
        vec![session.id.clone()]
    );

    sessions.revoke(&session.id, "alice").await.unwrap();
    assert!(sessions.sessions_for("alice").await.unwrap().is_empty());
}

#[quench_test::test]
async fn a_logout_clears_its_index_entry_too() {
    let sessions = sessions();
    let (_, token) = sessions.create("alice", 3600).await.unwrap();

    assert!(sessions.revoke_by_refresh_token(&token).await.unwrap());
    assert!(
        sessions.sessions_for("alice").await.unwrap().is_empty(),
        "a logged-out session left in the index would be reported as live"
    );
}

#[quench_test::test]
async fn a_client_holds_one_session_and_a_new_sign_in_replaces_it() {
    let sessions = sessions();
    let (first, old) = sessions
        .create_for_client("user", "app", 3600)
        .await
        .unwrap();
    let (second, _) = sessions
        .create_for_client("user", "app", 3600)
        .await
        .unwrap();
    let (other, _) = sessions
        .create_for_client("user", "other", 3600)
        .await
        .unwrap();

    assert!(!sessions.is_active(&first.id, "user").await.unwrap());
    assert!(sessions.rotate(&old, 3600).await.unwrap().is_none());
    assert!(sessions.is_active(&second.id, "user").await.unwrap());
    assert!(sessions.is_active(&other.id, "user").await.unwrap());
}

#[quench_test::test]
async fn a_young_client_token_is_kept_and_an_old_one_is_replaced() {
    let sessions = sessions();
    let (_, token) = sessions
        .create_for_client("user", "app", 3600)
        .await
        .unwrap();

    let (_, same) = sessions
        .rotate_with(&token, 3600, 3600, 3600)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        same, token,
        "younger than the limit: the same token goes on"
    );
    let (_, again) = sessions
        .rotate_with(&token, 3600, 3600, 3600)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again, token, "and it keeps working");

    let (_, next) = sessions
        .rotate_with(&token, 3600, 3600, 0)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(next, token, "older than the limit: replaced");
    assert!(
        sessions
            .rotate_with(&next, 3600, 3600, 3600)
            .await
            .unwrap()
            .is_some()
    );
}

#[quench_test::test]
async fn sessions_without_a_client_always_rotate() {
    let sessions = sessions();
    let (_, token) = sessions.create("user", 3600).await.unwrap();
    let (_, next) = sessions
        .rotate_with(&token, 3600, 3600, 3600)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(next, token);
}

#[quench_test::test]
async fn revoking_a_client_session_frees_its_slot() {
    let sessions = sessions();
    let (session, token) = sessions
        .create_for_client("user", "app", 3600)
        .await
        .unwrap();
    assert!(sessions.revoke(&session.id, "user").await.unwrap());
    assert!(
        sessions
            .rotate_with(&token, 3600, 3600, 3600)
            .await
            .unwrap()
            .is_none()
    );
    let (again, _) = sessions
        .create_for_client("user", "app", 3600)
        .await
        .unwrap();
    assert!(sessions.is_active(&again.id, "user").await.unwrap());
}
