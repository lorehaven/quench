//! Ed25519 signatures (RFC 8032).
//!
//! Signing keys are 32-byte seeds. They are stored as PKCS#8 DER. Every form
//! in the wild is read: v1 (48 bytes, seed only) and v2 (public key included,
//! as `ed25519-dalek`, `ring` and OpenSSL write it). What is written is
//! `ed25519-dalek`'s v2 layout byte for byte (83 bytes), so keys stored by this
//! crate stay readable by builds that still use it.

use std::fmt;

use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

/// The key bytes were not a usable Ed25519 key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError;

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a valid Ed25519 private key")
    }
}

impl std::error::Error for KeyError {}

/// PKCS#8 v1 header for an Ed25519 key; the 32-byte seed follows (read only).
#[cfg(test)]
const PKCS8_V1_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// PKCS#8 v2 header (version 1, public key attached); seed, then `V2_PUBLIC`
/// and the 32-byte public key follow.
const PKCS8_V2_PREFIX: [u8; 16] = [
    0x30, 0x51, 0x02, 0x01, 0x01, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];
const V2_PUBLIC: [u8; 3] = [0x81, 0x21, 0x00];

/// A private signing key.
pub struct SigningKey {
    seed: [u8; 32],
    pair: Ed25519KeyPair,
}

impl fmt::Debug for SigningKey {
    /// Never prints the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("public_key", &quench_core::hex::encode(self.public_key()))
            .finish_non_exhaustive()
    }
}

impl SigningKey {
    /// The key for a 32-byte seed.
    ///
    /// # Panics
    ///
    /// Never for a 32-byte seed: every seed is a valid Ed25519 key.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed)
            .expect("every 32-byte seed is a valid Ed25519 key");
        Self { seed: *seed, pair }
    }

    /// A fresh random key.
    #[must_use]
    pub fn generate() -> Self {
        Self::from_seed(&quench_core::random::bytes::<32>())
    }

    /// Reads an unencrypted PKCS#8 private key, v1 or v2.
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self, KeyError> {
        let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(der).map_err(|_| KeyError)?;
        // Both layouts put the seed right after the 16-byte header.
        let seed: [u8; 32] = der
            .get(16..48)
            .and_then(|s| s.try_into().ok())
            .ok_or(KeyError)?;
        Ok(Self { seed, pair })
    }

    /// The key as PKCS#8 v2 DER (83 bytes, `ed25519-dalek`'s layout).
    #[must_use]
    pub fn to_pkcs8_der(&self) -> Vec<u8> {
        let mut out = PKCS8_V2_PREFIX.to_vec();
        out.extend_from_slice(&self.seed);
        out.extend_from_slice(&V2_PUBLIC);
        out.extend_from_slice(&self.public_key());
        out
    }

    #[must_use]
    pub const fn seed(&self) -> &[u8; 32] {
        &self.seed
    }

    /// The 32-byte public key.
    ///
    /// # Panics
    ///
    /// Never: an Ed25519 public key is always 32 bytes.
    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        self.pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("an Ed25519 public key is 32 bytes")
    }

    /// The 64-byte signature of `message`.
    ///
    /// # Panics
    ///
    /// Never: an Ed25519 signature is always 64 bytes.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.pair
            .sign(message)
            .as_ref()
            .try_into()
            .expect("an Ed25519 signature is 64 bytes")
    }
}

/// Whether `signature` is `public_key`'s signature of `message`.
#[must_use]
pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool {
    UnparsedPublicKey::new(&ED25519, public_key)
        .verify(message, signature)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    fn h(text: &str) -> Vec<u8> {
        hex::decode(text).unwrap()
    }

    fn seed(text: &str) -> [u8; 32] {
        h(text).try_into().unwrap()
    }

    // RFC 8032 section 7.1, tests 1 and 2.
    #[test]
    fn rfc8032_vectors() {
        let key = SigningKey::from_seed(&seed(
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        ));
        assert_eq!(
            hex::encode(key.public_key()),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let signature = key.sign(b"");
        assert_eq!(
            hex::encode(signature),
            "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
        );
        assert!(verify(&key.public_key(), b"", &signature));

        let key = SigningKey::from_seed(&seed(
            "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
        ));
        assert_eq!(
            hex::encode(key.public_key()),
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"
        );
        assert_eq!(
            hex::encode(key.sign(&[0x72])),
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
        );
    }

    #[test]
    fn verification_rejects_tampering() {
        let key = SigningKey::generate();
        let signature = key.sign(b"message");
        let public = key.public_key();
        assert!(verify(&public, b"message", &signature));
        assert!(!verify(&public, b"messagE", &signature));
        let mut bad = signature;
        bad[0] ^= 1;
        assert!(!verify(&public, b"message", &bad));
        assert!(!verify(&public, b"message", &signature[..63]));
        assert!(!verify(&public[..31], b"message", &signature));
        assert!(!verify(
            &SigningKey::generate().public_key(),
            b"message",
            &signature
        ));
    }

    #[test]
    fn pkcs8_round_trips_in_the_dalek_v2_layout() {
        let key = SigningKey::generate();
        let der = key.to_pkcs8_der();
        assert_eq!(der.len(), 83);
        assert_eq!(&der[..16], &PKCS8_V2_PREFIX);
        assert_eq!(&der[48..51], &V2_PUBLIC);
        assert_eq!(&der[51..], &key.public_key());
        let back = SigningKey::from_pkcs8_der(&der).unwrap();
        assert_eq!(back.seed(), key.seed());
        assert_eq!(back.public_key(), key.public_key());
        assert_eq!(back.sign(b"x"), key.sign(b"x"));
    }

    #[test]
    fn pkcs8_v1_is_read() {
        let seed = [9u8; 32];
        let mut v1 = PKCS8_V1_PREFIX.to_vec();
        v1.extend_from_slice(&seed);
        let key = SigningKey::from_pkcs8_der(&v1).unwrap();
        assert_eq!(key.seed(), &seed);
        assert_eq!(key.public_key(), SigningKey::from_seed(&seed).public_key());
    }

    // The exact bytes ed25519-dalek 3 writes for the seed [42; 32].
    #[test]
    fn matches_the_bytes_ed25519_dalek_wrote() {
        let expected = "3051020101300506032b6570042204202a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a812100197f6b23e16c8532c6abc838facd5ea789be0c76b2920334039bfa8b3d368d61";
        let key = SigningKey::from_seed(&[42u8; 32]);
        assert_eq!(hex::encode(key.to_pkcs8_der()), expected);
        assert_eq!(
            SigningKey::from_pkcs8_der(&hex::decode(expected).unwrap())
                .unwrap()
                .seed(),
            &[42u8; 32]
        );
    }

    #[test]
    fn pkcs8_v2_is_read_too() {
        // What `ring` generates: v2, 83 bytes, public key included.
        let rng = ring::rand::SystemRandom::new();
        let generated = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let key = SigningKey::from_pkcs8_der(generated.as_ref()).unwrap();
        let reference = Ed25519KeyPair::from_pkcs8(generated.as_ref()).unwrap();
        assert_eq!(key.public_key().as_slice(), reference.public_key().as_ref());
        assert_eq!(key.sign(b"m").as_slice(), reference.sign(b"m").as_ref());
        // and re-exported in our own layout it signs identically
        assert_eq!(
            SigningKey::from_pkcs8_der(&key.to_pkcs8_der())
                .unwrap()
                .sign(b"m"),
            key.sign(b"m")
        );
    }

    #[test]
    fn rejects_malformed_private_keys() {
        for bad in [&b""[..], &[0u8; 48], &[0x30; 10], &PKCS8_V1_PREFIX[..]] {
            assert_eq!(SigningKey::from_pkcs8_der(bad).unwrap_err(), KeyError);
        }
        let mut short = SigningKey::generate().to_pkcs8_der();
        short.truncate(47);
        assert!(SigningKey::from_pkcs8_der(&short).is_err());
        assert!(KeyError.to_string().contains("Ed25519"));
    }

    #[test]
    fn debug_never_shows_the_seed() {
        let key = SigningKey::from_seed(&[7u8; 32]);
        let shown = format!("{key:?}");
        assert!(shown.contains("public_key"));
        assert!(!shown.contains("0707070707"), "{shown}");
    }
}
