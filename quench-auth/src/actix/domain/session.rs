//! Realm sessions.
//!
//! Sessions live in the cache store (Redis in a deployment, in-process for
//! tests), not in Postgres. They are ephemeral by nature: expiry is the store's
//! TTL, revocation is a delete, and there is nothing to sweep. Refresh-token
//! rotation is a single atomic take, so a replayed token cannot win a race.
//!
//! Four keys per session:
//!
//! ```text
//! session:{sid}         -> { username, refresh_hash }   TTL = refresh lifetime
//! refresh:{token_hash}  -> sid                          TTL = refresh lifetime
//! user:{username}       -> set of sids                  TTL = refresh lifetime
//! reuse:{old_hash}      -> { sid, username, token }      TTL = grace window
//! ```
//!
//! The third is an index, and the only key that is not reachable from a token.
//! It exists so that "end every session this user holds" is expressible, which
//! is what makes removing someone's permissions take effect now rather than
//! whenever their access token happens to expire. It is allowed to hold ids
//! that have since expired - [`SessionDb::revoke_all`] skips what is already
//! gone - but it must never *miss* one, which is why it is a set rather than a
//! JSON array read and written back.
//!
//! The fourth exists for a narrower reason than any of those three: a client
//! that rotates its refresh token and then never finds out - killed, or loses
//! connectivity, between gatehouse minting the next token and the response
//! reaching (or being persisted by) the caller - is left holding a token that
//! now looks identical to a stolen, already-used one. Without `reuse:*` that
//! client is locked out for good despite having done nothing wrong; a fresh
//! login is its only way back in. [`SessionDb::rotate`] answers a *second*
//! presentation of the same now-consumed token, within [`REUSE_GRACE_SECS`],
//! with the exact same result the first rotation already produced, rather
//! than treating it as invalid - and that recovery is itself a `take`, good
//! for one answer, not the whole window. This does cost a little of
//! single-use rotation's usual guarantee - a captured-in-flight token gets
//! one extra replay instead of zero - but the window is short and this is a
//! small deployment, not a target worth that trade against a real client
//! losing its
//! session to a crash.

use quench_cache::CacheStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

/// A session, as much of it as anything outside this module needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub username: String,
}

/// How long a just-consumed refresh token still answers [`SessionDb::rotate`]
/// with the token it was rotated to, instead of "no such token" - long enough
/// to cover a lost or never-persisted response, short enough that a captured
/// old token stays useless well before it would otherwise have mattered. See
/// this module's own doc comment for why this exists at all.
const REUSE_GRACE_SECS: u64 = 30;

pub struct SessionDb {
    store: CacheStore,
}

impl SessionDb {
    pub fn init(store: CacheStore) -> Arc<Self> {
        if store.is_shared() {
            tracing::info!("sessions are held in {}", store.topology());
        } else {
            tracing::warn!(
                "sessions are held in-process: a second replica will not see them, \
                 and they are lost on restart. Set REDIS_URL for a shared store."
            );
        }
        Arc::new(Self { store })
    }

    /// Reads `REDIS_URL`/`CACHE_URL`, falling back to an in-process store.
    pub async fn from_env() -> anyhow::Result<Arc<Self>> {
        let store = CacheStore::from_env("forge-session").await?;
        Ok(Self::init(store))
    }

    fn session_key(id: &str) -> String {
        format!("session:{id}")
    }

    fn refresh_key(token_hash: &str) -> String {
        format!("refresh:{token_hash}")
    }

    fn reuse_key(old_hash: &str) -> String {
        format!("reuse:{old_hash}")
    }

    fn user_key(username: &str) -> String {
        format!("user:{username}")
    }

    pub async fn create(&self, username: &str, ttl_secs: i64) -> anyhow::Result<(Session, String)> {
        let refresh_token = new_refresh_token();
        let refresh_hash = hash_refresh_token(&refresh_token);
        let session = Session {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
        };
        let ttl = ttl_secs.max(1) as u64;

        self.store
            .set(
                &Self::session_key(&session.id),
                json!({ "username": username, "refresh_hash": refresh_hash }),
                Some(ttl),
            )
            .await?;
        self.store
            .set(
                &Self::refresh_key(&refresh_hash),
                json!(session.id),
                Some(ttl),
            )
            .await?;
        self.store
            .add_to_set(&Self::user_key(username), &session.id, Some(ttl))
            .await?;

        tracing::info!("created session {} for {username}", session.id);
        Ok((session, refresh_token))
    }

    /// Exchanges a refresh token for a new one.
    ///
    /// The old token is consumed atomically, so a token presented twice in
    /// quick succession - a racing client, or an attacker replaying a stolen
    /// one - only ever triggers one real rotation. A *second* presentation of
    /// that exact same now-consumed token, though, gets the rotation's result
    /// handed back again rather than an error, for [`REUSE_GRACE_SECS`] - see
    /// this module's own doc comment for why.
    pub async fn rotate(
        &self,
        refresh_token: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<Option<(Session, String)>> {
        let old_hash = hash_refresh_token(refresh_token);
        let Some(session_id) = self.store.take(&Self::refresh_key(&old_hash)).await? else {
            return self.rotate_from_reuse(&old_hash).await;
        };
        let Some(session_id) = session_id.as_str().map(str::to_string) else {
            return Ok(None);
        };

        let Some(record) = self.store.get(&Self::session_key(&session_id)).await? else {
            // The session went away between the two reads: expired or revoked.
            return Ok(None);
        };
        let Some(username) = record
            .get("username")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            return Ok(None);
        };

        let next_token = new_refresh_token();
        let next_hash = hash_refresh_token(&next_token);
        let ttl = ttl_secs.max(1) as u64;

        self.store
            .set(
                &Self::session_key(&session_id),
                json!({ "username": username, "refresh_hash": next_hash }),
                Some(ttl),
            )
            .await?;
        self.store
            .set(
                &Self::refresh_key(&next_hash),
                json!(&session_id),
                Some(ttl),
            )
            .await?;
        // The session keys just had their lifetime extended; the index has to
        // follow or it expires first and the session becomes unrevokable.
        // `add_to_set` is idempotent, so re-adding is how the TTL is refreshed.
        self.store
            .add_to_set(&Self::user_key(&username), &session_id, Some(ttl))
            .await?;
        // The recovery path for whoever asks again with `refresh_token` before
        // finding out this already happened - see `rotate_from_reuse`.
        self.store
            .set(
                &Self::reuse_key(&old_hash),
                json!({
                    "session_id": session_id,
                    "username": username,
                    "refresh_token": next_token,
                }),
                Some(REUSE_GRACE_SECS),
            )
            .await?;

        Ok(Some((
            Session {
                id: session_id,
                username,
            },
            next_token,
        )))
    }

    /// The recovery path for a refresh token that `rotate` just found already
    /// consumed: within the grace window, hand back the exact token pair that
    /// consuming it produced, rather than treating this presentation as
    /// invalid. Outside the window - or if it was never rotated at all - this
    /// is `None`, same as today.
    ///
    /// `take`, not `get`: recovery is itself single-use, the same guarantee
    /// `rotate`'s own primary path gives the *current* token. Without this, a
    /// captured old token would stay replayable for the whole grace window
    /// instead of exactly the one recovery it exists for.
    async fn rotate_from_reuse(&self, old_hash: &str) -> anyhow::Result<Option<(Session, String)>> {
        let Some(reuse) = self.store.take(&Self::reuse_key(old_hash)).await? else {
            return Ok(None);
        };
        let (Some(session_id), Some(username), Some(refresh_token)) = (
            reuse.get("session_id").and_then(|v| v.as_str()),
            reuse.get("username").and_then(|v| v.as_str()),
            reuse.get("refresh_token").and_then(|v| v.as_str()),
        ) else {
            return Ok(None);
        };

        Ok(Some((
            Session {
                id: session_id.to_string(),
                username: username.to_string(),
            },
            refresh_token.to_string(),
        )))
    }

    /// Ends the session behind a refresh token. Takes effect everywhere at
    /// once, because every service reads the same store.
    pub async fn revoke_by_refresh_token(&self, refresh_token: &str) -> anyhow::Result<bool> {
        let hash = hash_refresh_token(refresh_token);
        let Some(session_id) = self.store.take(&Self::refresh_key(&hash)).await? else {
            return Ok(false);
        };
        if let Some(id) = session_id.as_str() {
            // Read the owner before dropping the record, or the index entry
            // cannot be found to remove.
            let username = self
                .store
                .get(&Self::session_key(id))
                .await?
                .and_then(|record| {
                    record
                        .get("username")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                });
            self.store.remove(&Self::session_key(id)).await?;
            if let Some(username) = username {
                self.store
                    .remove_from_set(&Self::user_key(&username), id)
                    .await?;
            }
            tracing::info!("revoked session {id}");
        }
        Ok(true)
    }

    /// Ends a session by id, provided it belongs to `username`.
    pub async fn revoke(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(record) = self.store.get(&Self::session_key(id)).await? else {
            // Still worth clearing: the index may hold an id whose session has
            // expired underneath it.
            self.store
                .remove_from_set(&Self::user_key(username), id)
                .await?;
            return Ok(false);
        };
        if record.get("username").and_then(|v| v.as_str()) != Some(username) {
            return Ok(false);
        }

        if let Some(hash) = record.get("refresh_hash").and_then(|v| v.as_str()) {
            self.store.remove(&Self::refresh_key(hash)).await?;
        }
        self.store.remove(&Self::session_key(id)).await?;
        self.store
            .remove_from_set(&Self::user_key(username), id)
            .await?;
        tracing::info!("revoked session {id}");
        Ok(true)
    }

    /// Ends every session `username` holds, and reports how many were live.
    ///
    /// This is what makes a permission change take effect immediately: without
    /// it a token keeps whatever scope it was minted with until it expires, up
    /// to `ACCESS_TOKEN_TTL_SECS`. Best effort per session - one that fails does
    /// not stop the rest, because leaving the remainder alive is the worse
    /// outcome.
    pub async fn revoke_all(&self, username: &str) -> anyhow::Result<usize> {
        let key = Self::user_key(username);
        let ids = self.store.set_members(&key).await?;
        let mut revoked = 0;

        for id in &ids {
            match self.revoke(id, username).await {
                Ok(true) => revoked += 1,
                Ok(false) => {}
                Err(err) => tracing::warn!("failed to revoke session {id} for {username}: {err}"),
            }
        }

        // Every member has been dealt with, so the index itself can go: a stale
        // key would keep answering with ids that no longer exist.
        self.store.remove(&key).await?;

        if revoked > 0 {
            tracing::info!("revoked {revoked} session(s) for {username}");
        }
        Ok(revoked)
    }

    /// Session ids currently indexed for a user. May include ids whose sessions
    /// have expired; callers that need certainty should check [`is_active`].
    ///
    /// [`is_active`]: Self::is_active
    pub async fn sessions_for(&self, username: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.store.set_members(&Self::user_key(username)).await?)
    }

    /// Whether the session is still usable. Called on every authenticated
    /// request, so it is a single point read.
    pub async fn is_active(&self, id: &str, username: &str) -> anyhow::Result<bool> {
        let Some(record) = self.store.get(&Self::session_key(id)).await? else {
            tracing::debug!("session {id} is gone (expired or revoked)");
            return Ok(false);
        };
        Ok(record.get("username").and_then(|v| v.as_str()) == Some(username))
    }
}

fn new_refresh_token() -> String {
    format!("{}.{}", Uuid::new_v4(), Uuid::new_v4())
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
