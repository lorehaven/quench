---
title: "API: prelude, JWT, JWKS, signing"
parent: quench-auth
grand_parent: Crates
nav_order: 3
---

## `quench_auth` (crate root)

```rust
pub mod domain;
#[cfg(feature = "http")] pub mod http;
pub mod prelude;
```

## `quench_auth::prelude`

```rust
pub use crate::domain::auth::{Actions, Permissions, Role, User, UserDb};
pub use crate::domain::jwt::{Claims, JwtConfig};
pub use crate::domain::realm;
pub use crate::domain::session::{Session, SessionDb};
pub mod jwt { pub use crate::domain::jwt::*; }
#[cfg(feature = "http")]
pub mod routers { pub use crate::http::routers::*; }   // so prelude::routers::ui::...
```

`KeyResolver`, `KeySigner` are reachable as `prelude::jwt::{KeyResolver, KeySigner}`.

## Module `domain`

`domain` re-exports nothing; it declares the public modules `auth`, `jwks`, `jwt`, `percent`, `realm`, `session`, `signing`, `sso_client`. With the `http` feature, `quench_auth::http::domain::{auth, jwks, jwt, realm, session, signing}` re-export the same modules.

## Module `domain::jwt`

Tokens are JWTs signed with **EdDSA (Ed25519)**; the header carries a `kid`. `JwtError` below means `quench_crypto::jwt::Error` (with `kind() -> &ErrorKind`).

### `KeyResolver`

```rust
#[async_trait]
pub trait KeyResolver: Send + Sync {
    async fn resolve(&self, kid: &str) -> Option<DecodingKey>;
}
```
Resolves the public key a token's `kid` was signed with. Implemented by `JwksVerifier` (relying parties) and, in gatehouse, by its key store. `None` means unknown key.

### `KeySigner`

```rust
#[async_trait]
pub trait KeySigner: Send + Sync {
    async fn active(&self) -> Option<(String, EncodingKey)>;
}
```
The `kid` and encoding key new tokens are signed with. Only gatehouse constructs a `JwtConfig` with a signer.

### `JwtConfig`

```rust
#[derive(Clone)]
pub struct JwtConfig {
    pub service_name: String,
    pub audiences: Vec<String>,
    pub realm: String,
    pub auth_enabled: bool,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    /* private: keys: Arc<dyn KeyResolver>, signer: Option<Arc<dyn KeySigner>> */
}
```
`Debug` prints the public fields only. Every constructor reads the environment (see [Configuration](configuration.md)):

| Field | Env | Default |
|---|---|---|
| `service_name` | `SERVICE_NAME` | `"service"` |
| `realm` | `SERVICE_REALM` | `"https://localhost:8698/token"` |
| `audiences` | `SERVICE_AUDIENCES`, comma-separated, items trimmed, empties dropped | `[service_name]` when empty |
| `auth_enabled` | `SERVICE_AUTH_ENABLED`, parsed with `bool::from_str` (`true`/`false` exactly) | `false`; unparsable also `false` |
| `access_token_ttl_secs` | `ACCESS_TOKEN_TTL_SECS` | `900`; unparsable also `900` |
| `refresh_token_ttl_secs` | `REFRESH_TOKEN_TTL_SECS` | `604800`; unparsable also `604800` |

```rust
pub async fn init() -> Self
```
Relying-party construction: a `JwksVerifier::from_env()` resolver and no signer. **Panics** with `failed to set up the JWKS verifier (is GATEHOUSE_URL set?)` if the verifier cannot be built (`GATEHOUSE_URL` unset, HTTP client or cache setup failure).

```rust
pub fn init_signing<T>(authority: Arc<T>) -> Self where T: KeyResolver + KeySigner + 'static
```
Gatehouse construction: `authority` both resolves and signs.

```rust
#[must_use] pub fn for_tests() -> Self
#[must_use] pub fn for_tests_with_signing() -> Self
```
Public (not `cfg(test)`) so other crates' tests can use them. `for_tests` resolves no key and cannot sign. `for_tests_with_signing` generates one in-memory Ed25519 key with `kid = "test"` and signs and verifies with it.

```rust
pub async fn encode_claims(&self, claims: &Claims) -> Result<String, JwtError>
```
Signs `claims` with header `alg = EdDSA` and the signer's `kid`. Errors: no signer or no active key is `ErrorKind::InvalidKeyFormat`; otherwise signing errors.

```rust
pub async fn decode_claims(&self, token: &str) -> Result<Claims, JwtError>
```
Reads the header, requires a `kid` (`InvalidToken` if absent), resolves the key (`InvalidKeyFormat` if unknown), verifies signature and `exp` with `Validation::new(EdDSA)` (library default leeway of 60 s), requires `iat` and `exp`, does **not** validate `aud` (use `Claims::allows`), and finally rejects `iat` later than now plus the leeway with `ErrorKind::ImmatureSignature`.

```rust
pub async fn issue_access_token(&self, username: String, scope: String, session_id: Option<String>) -> Result<String, JwtError>
pub async fn issue_access_token_for(&self, username: String, audiences: Vec<String>, scope: String, session_id: Option<String>) -> Result<String, JwtError>
```
Issue a token for all of `self.audiences`, or for the given audiences (taken as given: apply `narrow_audiences` first). `exp = now + access_token_ttl_secs`.

```rust
#[must_use] pub fn narrow_audiences(&self, wanted: &[String]) -> Vec<String>
```
`self.audiences` filtered to those in `wanted`, in `self.audiences` order. A caller cannot widen a token past `SERVICE_AUDIENCES`.

### `Claims`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)] pub aud: Vec<String>,
    pub scope: String,
    pub exp: usize,
    pub iat: usize,
    #[serde(default)] pub sid: Option<String>,
}
```
`aud` is a JSON array of service names. `scope` is a list of entries separated by spaces or commas: roles (`admin`, `user`, `service`; never contain `:`) and permissions `service:action` (`sage:read`). `sid` is the session id; machine tokens have none.

```rust
#[must_use] pub fn for_audiences(sub: String, aud: Vec<String>, scope: String, sid: Option<String>, duration_secs: i64) -> Self
```
`iat = now`, `exp = now + duration_secs` (Unix seconds; negative values clamp to 0).

```rust
#[must_use] pub fn allows(&self, service_name: &str) -> bool
pub fn roles(&self) -> Vec<String>
#[must_use] pub fn has_role(&self, role: &str) -> bool
#[must_use] pub fn permissions(&self) -> Permissions
#[must_use] pub fn can(&self, service: &str, action: &str) -> bool
#[must_use] pub fn has_wildcard(&self) -> bool
```
* `allows`: `aud` contains `service_name` exactly.
* `roles`: every non-empty entry of `scope` split on space and comma (roles *and* permission entries; the name is historical).
* `has_role`: some entry without `:` equals `role`, ASCII case-insensitive.
* `permissions`: entries `service:action` folded into `{service: {action, ..}}` (split at the first `:`).
* `can`: `has_wildcard()` or `permissions()[service]` contains `action`.
* `has_wildcard`: some entry without `:` parses as a `Role` with `is_wildcard()` (`admin`, `service`).

## Module `domain::jwks`

### `JwksVerifier`

```rust
#[derive(Debug)]
pub struct JwksVerifier { /* http client, jwks_url, cache: CacheStore */ }
impl KeyResolver for JwksVerifier {}
```
Verifies tokens against gatehouse's published keys.

```rust
pub async fn from_env() -> quench_core::Result<Self>
pub async fn new(gatehouse_base_url: &str, tls_verify: bool) -> quench_core::Result<Self>
```
`from_env` reads `GATEHOUSE_URL` (empty or unset is an error: `GATEHOUSE_URL must be set - every service verifies realm tokens against gatehouse's JWKS`) and `GATEHOUSE_TLS_VERIFY` (default `true`; unparsable is `true`) and calls `new`. `new` builds an HTTP client (5 s timeout, `danger_accept_invalid_certs(!tls_verify)`) and a `CacheStore::from_env("jwks")` (shared through Redis when `CACHE_URL`/`REDIS_URL` is set). The URL fetched is `<base without trailing />/.well-known/jwks.json`.

`resolve(kid)`: cache key `kid:<kid>` (store key prefix `jwks`). A cached string is the key's base64url Ed25519 public key (`x`); a cached `null` is a remembered miss; otherwise the whole JWKS is fetched and every key is cached for 3600 s; if `kid` is not in the set a `null` is cached for 30 s. Any HTTP, JSON or decoding failure resolves to `None` (and nothing is cached on HTTP failure). Only the `kid` and `x` members of each JWK are read.

## Module `domain::signing`

```rust
pub struct GeneratedSigningKey {
    pub private_key_der: Vec<u8>,   // PKCS8 DER
    pub public_key: Vec<u8>,        // raw 32 bytes
}
```
`Debug` redacts the private key.

```rust
#[must_use] pub fn generate_signing_key() -> GeneratedSigningKey
pub fn encoding_key(private_key_der: &[u8]) -> Result<EncodingKey, quench_crypto::jwt::Error>
#[must_use] pub fn decoding_key(public_key: &[u8]) -> DecodingKey
```
Generates an Ed25519 key; builds the signing key from PKCS8 DER (either version; unreadable input is an error); builds the verification key from the raw 32 public bytes.
