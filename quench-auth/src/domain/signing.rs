//! Ed25519 key generation, and the byte shapes gatehouse persists.
//!
//! Kept in one place so the key encoding stays an implementation detail of
//! this crate: gatehouse's `SigningKeys` (`docker/gatehouse-service/src/keys.rs`)
//! only ever handles opaque `Vec<u8>` and the key types this module hands back.

use quench_crypto::ed25519::SigningKey;
use quench_crypto::jwt::{DecodingKey, EncodingKey, Error};

pub struct GeneratedSigningKey {
    /// PKCS8 DER - what `encoding_key` expects back.
    pub private_key_der: Vec<u8>,
    /// Raw 32 bytes - what `decoding_key` and a JWK's `x` expect.
    pub public_key: Vec<u8>,
}

// Hand-written so the private key can never end up in a log line.
impl std::fmt::Debug for GeneratedSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedSigningKey")
            .field("private_key_der", &"<redacted>")
            .field("public_key", &self.public_key)
            .finish()
    }
}

#[must_use]
pub fn generate_signing_key() -> GeneratedSigningKey {
    let signing_key = SigningKey::generate();
    GeneratedSigningKey {
        private_key_der: signing_key.to_pkcs8_der(),
        public_key: signing_key.public_key().to_vec(),
    }
}

/// The signing key for stored PKCS8 DER (either encoding version).
pub fn encoding_key(private_key_der: &[u8]) -> Result<EncodingKey, Error> {
    EncodingKey::from_ed_der(private_key_der)
}

#[must_use]
pub fn decoding_key(public_key: &[u8]) -> DecodingKey {
    DecodingKey::from_ed_der(public_key)
}
