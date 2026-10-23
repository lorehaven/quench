//! Cryptography for quench services, on top of `ring` (the one crypto backend
//! allowed in this workspace; `rustls` already uses it).
//!
//! Always built: `hash` (SHA-256/384/512), `hmac`, `ct` (constant-time
//! comparison) and `pbkdf2`. Behind features, all on by default: `ed25519`
//! (signing keys in PKCS#8 v1 and v2, verification), `rsa` (PKCS#1 v1.5
//! verification), `argon2` (Argon2id password hashing), `totp` (HOTP/TOTP one-time
//! passwords), `aead` (ChaCha20-Poly1305 and XChaCha20-Poly1305) and `jwt` (JSON Web
//! Tokens, which brings in `ed25519` and `rsa`).

#[cfg(feature = "aead")]
pub mod aead;
#[cfg(feature = "age")]
pub mod age;
#[cfg(feature = "argon2")]
pub mod argon2;
#[cfg(feature = "argon2")]
mod blake2b;
pub mod ct;
#[cfg(feature = "ed25519")]
pub mod ed25519;
pub mod hash;
pub mod hkdf;
pub mod hmac;
#[cfg(feature = "jwt")]
pub mod jwt;
pub mod pbkdf2;
#[cfg(feature = "rsa")]
pub mod rsa;
#[cfg(feature = "totp")]
pub mod totp;
#[cfg(feature = "x25519")]
pub mod x25519;
