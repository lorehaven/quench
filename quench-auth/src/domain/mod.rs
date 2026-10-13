//! Framework-agnostic auth domain logic - JWT encode/decode, user/session
//! storage, realm cookie/URL conventions, Ed25519 key handling. None of
//! this depends on an HTTP framework; `http::domain::*` re-exports from
//! here.

pub mod auth;
pub mod jwks;
pub mod jwt;
pub mod percent;
pub mod realm;
pub mod session;
pub mod signing;
pub mod sso_client;
