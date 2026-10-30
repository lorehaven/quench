---
title: "API: users, roles, sessions"
parent: quench-auth
grand_parent: Crates
nav_order: 5
---

## Module `domain::auth`

### `Role`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role { Admin, User, Service }
```
```rust
#[must_use] pub const fn is_wildcard(&self) -> bool          // Admin | Service
#[must_use] pub const fn as_str(&self) -> &'static str       // "admin" | "user" | "service"
#[must_use] pub fn parse(value: &str) -> Option<Self>        // trims, ASCII-lowercases; unknown is None
impl Display for Role                                         // as_str()
```
A wildcard role grants every permission on every service. Managing users is a separate concern for `Admin` alone, decided in gatehouse.

### Types

```rust
pub type Actions = BTreeSet<String>;
pub type Permissions = BTreeMap<String, Actions>;   // {service: {action, ...}}
```
Action names are free strings (`"read"`, `"write"`, `"launch"`); there is no ordering or implication between them.

### `User`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password: String,                 // Argon2 hash
    pub roles: Value,                     // JSON array of lowercase role names
    pub permissions: Value,               // JSON object {service: [action, ...]}
    pub email: Option<String>,
    pub email_verified_at: Option<Time>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub title: Option<String>,
    pub created_at: Time,
    pub last_login_at: Option<Time>,
    pub disabled_at: Option<Time>,
    pub password_changed_at: Option<Time>,
    pub mfa_enabled: bool,
    pub mfa_secret: Option<String>,       // never verified by this crate
    pub failed_login_attempts: i32,
    pub locked_until: Option<Time>,
    pub timezone: Option<String>,
    pub preferred_locale: Option<String>,
}
```
`Time` is `quench_core::time::Time`; `Value` is `quench_serde::Value`. `User` implements `quench_db::Model`: `table_name()` is `"<auth schema>.users"` (schema from `realm::auth_schema()`, default `auth`), primary key `username`, and `columns()` is the 19 field names above in declaration order.

```rust
pub fn new(username: String, password: String, roles: Vec<Role>, permissions: Permissions, email: Option<String>) -> quench_core::Result<Self>
pub fn hash_password(password: &str) -> quench_core::Result<String>
#[must_use] pub fn verify_password(&self, password: &str) -> bool
```
`new` hashes `password` with Argon2 (`quench_crypto::argon2`, default `Params`), sets `created_at` to now and all other optional/flag fields to `None`/`false`/`0`. `hash_password` errors as `failed to hash password: ..`. `verify_password` is false on any verification error; hashes made with other Argon2 parameters still verify.

```rust
#[must_use] pub fn get_roles(&self) -> Vec<Role>
#[must_use] pub fn get_permissions(&self) -> Permissions
pub fn has_wildcard(&self) -> bool
#[must_use] pub fn can(&self, service: &str, action: &str) -> bool
```
`get_roles` / `get_permissions` deserialize the JSON columns; unreadable data reads as empty (no roles, no grants). `can` is true for a wildcard role or a matching grant.

```rust
#[must_use] pub const fn is_disabled(&self) -> bool          // disabled_at.is_some()
#[must_use] pub fn is_locked(&self) -> bool                  // locked_until > now
pub fn record_failed_login(&mut self, max_attempts: i32, lockout_duration: Duration)
pub fn record_successful_login(&mut self)
```
`record_failed_login` increments `failed_login_attempts`; when it reaches `max_attempts` it sets `locked_until = now + lockout_duration` and resets the counter to 0. `record_successful_login` resets the counter and sets `last_login_at`. (`Duration` is `quench_core::time::Duration`.)

### `UserDb`

```rust
#[derive(Debug)]
pub enum UserDb { Base { repo: Repository<User> } }
```
Read-only access to the realm's users (HTTP Basic machine auth, warehouse's registry token endpoint).

```rust
pub async fn init(db: Db) -> Arc<Self>
pub async fn get_user(&self, username: &str) -> Option<User>
pub async fn validate(&self, username: &str, password: &str) -> Option<User>
```
`get_user` reads one row; a read error is `None`. `validate` returns the user only if it exists, is not disabled, is not locked and the password verifies (Argon2 runs in `tokio::task::spawn_blocking`). It does not count failures toward a lockout.

## Module `domain::session`

Sessions live in a `CacheStore` (see [Internals](internals.md#session-keys) for the key layout). Errors are `quench_core::Result` (a `CacheError` converts into it).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session { pub id: String, pub username: String }

#[derive(Debug)]
pub struct SessionDb { /* store: CacheStore */ }
```

```rust
#[must_use] pub fn init(store: CacheStore) -> Arc<Self>
pub async fn from_env() -> quench_core::Result<Arc<Self>>
```
`init` logs where sessions are held (info if shared, a warning if in-process). `from_env` uses `CacheStore::from_env("forge-session")`.

```rust
pub async fn create(&self, username: &str, ttl_secs: i64) -> quench_core::Result<(Session, String)>
```
New session id (UUID v4) and refresh token (`<uuid>.<uuid>`). The TTL is `ttl_secs` clamped to at least 1 second. Returns the session and the plaintext refresh token (only its SHA-256 hex is stored).

```rust
pub async fn rotate(&self, refresh_token: &str, ttl_secs: i64) -> quench_core::Result<Option<(Session, String)>>
```
Atomically consumes the refresh token (`take`) and issues the next one for the same session, extending all lifetimes to `ttl_secs`. Returns `None` for an unknown or consumed token, or if the session vanished. A token consumed within the last 30 seconds (`REUSE_GRACE_SECS`) answers once more with the same result as the first rotation (the recovery is itself a `take`, good for one answer).

```rust
pub async fn revoke_by_refresh_token(&self, refresh_token: &str) -> quench_core::Result<bool>
pub async fn revoke(&self, id: &str, username: &str) -> quench_core::Result<bool>
pub async fn revoke_all(&self, username: &str) -> quench_core::Result<usize>
```
`revoke_by_refresh_token`: `true` if the token was known. `revoke`: ends session `id` only if it belongs to `username`; `false` when missing or owned by someone else (a missing session still clears the index entry). `revoke_all`: ends every indexed session of the user (best effort per session; failures are logged), removes the index, returns how many were live.

```rust
pub async fn sessions_for(&self, username: &str) -> quench_core::Result<Vec<String>>
pub async fn is_active(&self, id: &str, username: &str) -> quench_core::Result<bool>
```
`sessions_for` lists indexed ids (may include expired sessions). `is_active` is a single point read: true when the session exists and belongs to `username`.
