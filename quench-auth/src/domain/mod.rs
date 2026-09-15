//! Framework-agnostic auth domain logic - JWT encode/decode, user/session
//! storage, realm cookie/URL conventions, Ed25519 key handling. None of
//! this ever touched actix-web; it lived under `actix/domain` only because
//! that was the only framework this crate had. `actix::domain::*` and
//! `http::domain::*` both re-export from here.

pub mod auth;
pub mod jwks;
pub mod jwt;
pub mod realm;
pub mod session;
pub mod signing;
pub mod sso_client;
