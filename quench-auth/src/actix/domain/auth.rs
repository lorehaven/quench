use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{DateTime, Duration, Utc};
use quench_db::prelude::{Crud, Db, Model, Repository};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    User,
    Service,
}

impl Role {
    /// Whether the role grants every permission on every service.
    ///
    /// `Admin` administers the realm; `Service` is the machine-to-machine
    /// identity, which has to reach whatever the estate happens to contain.
    /// Both are wildcards so that adding a service never means re-granting
    /// anything, and neither has entries written into `permissions`: an
    /// enumerated grant list would go stale the moment `SERVICE_AUDIENCES`
    /// changed.
    ///
    /// Note this is about *service access* only. Managing users is `Admin`
    /// alone - see gatehouse's `AdminClaims`.
    pub const fn is_wildcard(&self) -> bool {
        matches!(self, Self::Admin | Self::Service)
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
            Self::Service => "service",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "admin" => Some(Self::Admin),
            "user" => Some(Self::User),
            "service" => Some(Self::Service),
            _ => None,
        }
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The set of action names a service declares it supports.
///
/// A grant is holding one of these names for a service - `"read"`, `"write"`,
/// or something a service defines for itself, like switchboard's `"launch"`.
/// There is deliberately no ordering or implication between them (`"write"`
/// does not imply `"read"`): the catalog that defines which names exist per
/// service (`docker/gatehouse-service/config/permissions.toml`) is what makes
/// this safe - an admin checks the boxes they mean, and there is nothing
/// magic underneath for a reader to get wrong. `quench-auth` has no notion of
/// which action names exist for which service; that lives in gatehouse's
/// catalog, and is checked at grant time, not at read time.
pub type Actions = BTreeSet<String>;

/// A user's grants, as `{service: {action, action, ...}}`.
pub type Permissions = BTreeMap<String, Actions>;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub username: String,
    pub password: String,
    pub roles: serde_json::Value,
    /// `{"sage": ["read", "write"], "switchboard": ["launch"]}`. Empty for a
    /// wildcard role, which grants everything without enumerating it.
    pub permissions: serde_json::Value,
    /// Set by self-service registration; `None` for an admin-created account,
    /// which has no reason to collect one.
    pub email: Option<String>,
    /// When the address in `email` was confirmed via a verification link.
    /// `None` either because there is no address, or because it has not been
    /// confirmed yet - the two are told apart by checking `email` itself.
    pub email_verified_at: Option<DateTime<Utc>>,

    // ---------------------------------------------------------------------
    // Profile - self-service or admin-set, never required for login.
    // ---------------------------------------------------------------------
    pub display_name: Option<String>,
    /// A link to an externally-hosted image, not a warehouse-stored upload.
    pub avatar_url: Option<String>,
    pub title: Option<String>,

    // ---------------------------------------------------------------------
    // Lifecycle
    // ---------------------------------------------------------------------
    /// Backfilled to the migration's own run time for accounts that existed
    /// before this column did - see
    /// `docker/foundry-service/migrations/auth/0008-user-lifecycle.toml`.
    /// Not the account's real creation date for those rows, but a value from
    /// here on is still better than none.
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    /// A disabled account keeps its row (and everything it's attached to
    /// elsewhere - issues reported, runs triggered) but cannot authenticate;
    /// see [`User::is_disabled`].
    pub disabled_at: Option<DateTime<Utc>>,
    pub password_changed_at: Option<DateTime<Utc>>,

    // ---------------------------------------------------------------------
    // Security
    // ---------------------------------------------------------------------
    pub mfa_enabled: bool,
    /// Encrypted at rest by gatehouse the same way its own signing keys are
    /// (`docker/gatehouse-service/src/keys.rs`) - this crate never sees the
    /// plaintext secret or verifies a code itself. Interactive login only:
    /// no relying party's machine-to-machine path challenges for one.
    pub mfa_secret: Option<String>,
    pub failed_login_attempts: i32,
    /// Set once `failed_login_attempts` crosses the caller's configured
    /// threshold; see [`User::record_failed_login`].
    pub locked_until: Option<DateTime<Utc>>,

    // ---------------------------------------------------------------------
    // Contact / locale
    // ---------------------------------------------------------------------
    pub timezone: Option<String>,
    pub preferred_locale: Option<String>,
}

impl User {
    pub fn new(
        username: String,
        password: String,
        roles: Vec<Role>,
        permissions: Permissions,
        email: Option<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            username,
            password: Self::hash_password(&password)?,
            roles: serde_json::to_value(roles).unwrap(),
            permissions: serde_json::to_value(permissions).unwrap(),
            email,
            email_verified_at: None,
            display_name: None,
            avatar_url: None,
            title: None,
            created_at: Utc::now(),
            last_login_at: None,
            disabled_at: None,
            password_changed_at: None,
            mfa_enabled: false,
            mfa_secret: None,
            failed_login_attempts: 0,
            locked_until: None,
            timezone: None,
            preferred_locale: None,
        })
    }

    pub fn hash_password(password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|err| anyhow::anyhow!("failed to hash password: {err}"))?
            .to_string())
    }

    pub fn verify_password(&self, password: &str) -> bool {
        let Ok(hash) = PasswordHash::new(&self.password) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    }

    pub fn get_roles(&self) -> Vec<Role> {
        serde_json::from_value(self.roles.clone()).unwrap_or_default()
    }

    /// The grants written against this user, ignoring any wildcard role.
    ///
    /// This is what an editor shows and what a `PUT` replaces. For "may they
    /// actually do X", use [`User::can`] - it is the one that honours the
    /// wildcard. A row that will not parse as `{service: [action, ...]}`
    /// reads as no grants at all rather than failing the read: one bad row
    /// should not lock a user out of everything, and "no access" is the safe
    /// direction for that failure to go.
    pub fn get_permissions(&self) -> Permissions {
        serde_json::from_value(self.permissions.clone()).unwrap_or_default()
    }

    pub fn has_wildcard(&self) -> bool {
        self.get_roles().iter().any(Role::is_wildcard)
    }

    /// Whether this user may perform `action` on `service`. A wildcard role
    /// short-circuits, which is why `get_permissions` stays empty for one -
    /// there is nothing to enumerate.
    pub fn can(&self, service: &str, action: &str) -> bool {
        if self.has_wildcard() {
            return true;
        }
        self.get_permissions()
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }

    /// Whether an admin has turned this account off. Distinct from deletion -
    /// the row (and everything elsewhere that references the username) stays
    /// intact, it just can no longer authenticate anywhere in the realm.
    pub fn is_disabled(&self) -> bool {
        self.disabled_at.is_some()
    }

    /// Whether a lockout from repeated failed logins is still in effect.
    pub fn is_locked(&self) -> bool {
        self.locked_until.is_some_and(|until| until > Utc::now())
    }

    /// Call after a failed password check. Past `max_attempts`, locks the
    /// account for `lockout_duration` and resets the counter - so the count
    /// a caller sees is always "failures since the last lock", not a number
    /// that climbs forever. The threshold and duration are policy the caller
    /// configures (gatehouse's own login flow); this crate has no opinion on
    /// what they should be.
    pub fn record_failed_login(&mut self, max_attempts: i32, lockout_duration: Duration) {
        self.failed_login_attempts += 1;
        if self.failed_login_attempts >= max_attempts {
            self.locked_until = Some(Utc::now() + lockout_duration);
            self.failed_login_attempts = 0;
        }
    }

    /// Call after a successful password check (and, if this account has MFA
    /// enabled, after the code too) - clears any accumulated failure count
    /// and stamps when the session started.
    pub fn record_successful_login(&mut self) {
        self.failed_login_attempts = 0;
        self.last_login_at = Some(Utc::now());
    }
}

impl Model for User {
    fn table_name() -> String {
        format!("{}.users", crate::actix::domain::realm::auth_schema())
    }

    fn columns() -> Vec<&'static str> {
        vec![
            "username",
            "password",
            "roles",
            "permissions",
            "email",
            "email_verified_at",
            "display_name",
            "avatar_url",
            "title",
            "created_at",
            "last_login_at",
            "disabled_at",
            "password_changed_at",
            "mfa_enabled",
            "mfa_secret",
            "failed_login_attempts",
            "locked_until",
            "timezone",
            "preferred_locale",
        ]
    }

    fn primary_key_name() -> String {
        "username".to_string()
    }
}

pub enum UserDb {
    Base { repo: Repository<User> },
}

/// Read-only access to the realm's users.
///
/// Relying parties need this for the machine-to-machine Basic auth path and,
/// in warehouse's case, the registry token endpoint. Creating users is
/// gatehouse's job, so there is deliberately no write method here.
impl UserDb {
    pub async fn init(db: Db) -> Arc<Self> {
        Arc::new(Self::Base {
            repo: db.repository::<User>(),
        })
    }

    pub async fn get_user(&self, username: &str) -> Option<User> {
        let Self::Base { repo } = self;
        repo.read(username).await.unwrap_or(None)
    }

    /// Checks the password and, first, that the account is allowed to
    /// authenticate at all. `disabled_at`/`locked_until` gate every relying
    /// party through this one check, not just gatehouse's own login - a
    /// locked-out account should not be able to authenticate to warehouse's
    /// registry either. What this deliberately does *not* do is count this
    /// failure toward a future lockout: `UserDb` has no write access (see
    /// the type's own doc comment), so only gatehouse's own interactive
    /// login - which does have one, via `realm.rs` - tracks attempts and
    /// calls [`User::record_failed_login`]/[`User::record_successful_login`]
    /// itself. MFA is the same story: it's an interactive second step
    /// gatehouse's login page asks for, never something a Basic-auth caller
    /// through here is challenged with.
    pub async fn validate(&self, username: &str, password: &str) -> Option<User> {
        tracing::debug!("UserDb::validate: Looking up user: {}", username);
        let user = self.get_user(username).await?;
        if user.is_disabled() {
            tracing::warn!("UserDb::validate: account is disabled: {}", username);
            return None;
        }
        if user.is_locked() {
            tracing::warn!("UserDb::validate: account is locked: {}", username);
            return None;
        }
        tracing::debug!(
            "UserDb::validate: User found, verifying password for: {}",
            username
        );
        let verify_user = user.clone();
        let password = password.to_string();
        let verified = tokio::task::spawn_blocking(move || verify_user.verify_password(&password))
            .await
            .ok()?;
        if verified {
            tracing::debug!("UserDb::validate: Password verified for user: {}", username);
            Some(user)
        } else {
            tracing::warn!("UserDb::validate: Invalid password for user: {}", username);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> User {
        User::new(
            "someone".to_string(),
            "password".to_string(),
            vec![Role::User],
            Permissions::new(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_user_is_neither_disabled_nor_locked() {
        let user = user();
        assert!(!user.is_disabled());
        assert!(!user.is_locked());
    }

    #[test]
    fn failed_logins_below_the_threshold_do_not_lock() {
        let mut user = user();
        for _ in 0..4 {
            user.record_failed_login(5, Duration::minutes(15));
        }
        assert_eq!(user.failed_login_attempts, 4);
        assert!(!user.is_locked());
    }

    #[test]
    fn the_nth_failed_login_locks_and_resets_the_counter() {
        let mut user = user();
        for _ in 0..5 {
            user.record_failed_login(5, Duration::minutes(15));
        }
        assert!(user.is_locked());
        // The count is "failures since the last lock", not a running total -
        // it must not still read 5 once a lock has just been applied.
        assert_eq!(user.failed_login_attempts, 0);
    }

    #[test]
    fn a_lock_expires_on_its_own_once_the_duration_passes() {
        let mut user = user();
        user.locked_until = Some(Utc::now() - Duration::seconds(1));
        assert!(!user.is_locked());
    }

    #[test]
    fn a_successful_login_clears_the_failure_count_and_stamps_last_login() {
        let mut user = user();
        user.record_failed_login(5, Duration::minutes(15));
        user.record_failed_login(5, Duration::minutes(15));
        assert!(user.last_login_at.is_none());

        user.record_successful_login();

        assert_eq!(user.failed_login_attempts, 0);
        assert!(user.last_login_at.is_some());
    }

    #[test]
    fn disabled_is_driven_by_disabled_at_alone() {
        let mut user = user();
        assert!(!user.is_disabled());
        user.disabled_at = Some(Utc::now());
        assert!(user.is_disabled());
    }
}
