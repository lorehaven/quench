//! SHA-2 digests.

use ring::digest;

/// A SHA-2 variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    const fn ring(self) -> &'static digest::Algorithm {
        match self {
            Self::Sha256 => &digest::SHA256,
            Self::Sha384 => &digest::SHA384,
            Self::Sha512 => &digest::SHA512,
        }
    }

    /// Digest length in bytes.
    #[must_use]
    pub const fn output_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

/// The digest of `data`.
#[must_use]
pub fn digest(algorithm: Algorithm, data: impl AsRef<[u8]>) -> Vec<u8> {
    digest::digest(algorithm.ring(), data.as_ref())
        .as_ref()
        .to_vec()
}

#[must_use]
pub fn sha256(data: impl AsRef<[u8]>) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(digest::digest(&digest::SHA256, data.as_ref()).as_ref());
    out
}

#[must_use]
pub fn sha384(data: impl AsRef<[u8]>) -> [u8; 48] {
    let mut out = [0u8; 48];
    out.copy_from_slice(digest::digest(&digest::SHA384, data.as_ref()).as_ref());
    out
}

#[must_use]
pub fn sha512(data: impl AsRef<[u8]>) -> [u8; 64] {
    let mut out = [0u8; 64];
    out.copy_from_slice(digest::digest(&digest::SHA512, data.as_ref()).as_ref());
    out
}

/// SHA-1. Broken for collision resistance: only for protocols that mandate it
/// (the WebSocket handshake, RFC 6455), never for new designs or signatures.
#[must_use]
pub fn sha1_legacy(data: impl AsRef<[u8]>) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(digest::digest(&digest::SHA1_FOR_LEGACY_USE_ONLY, data.as_ref()).as_ref());
    out
}

/// An incremental hasher, for input that arrives in pieces.
#[derive(Clone)]
pub struct Hasher(digest::Context);

impl std::fmt::Debug for Hasher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hasher").finish_non_exhaustive()
    }
}

impl Hasher {
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Self {
        Self(digest::Context::new(algorithm.ring()))
    }

    pub fn update(&mut self, data: impl AsRef<[u8]>) {
        self.0.update(data.as_ref());
    }

    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.0.finish().as_ref().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // FIPS 180-4 examples.
    #[quench_test::test]
    fn nist_vectors() {
        assert_eq!(
            hex::encode(sha256("")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex::encode(sha256("abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex::encode(sha256(
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex::encode(sha384("abc")),
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"
        );
        assert_eq!(
            hex::encode(sha512("abc")),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    #[quench_test::test]
    fn sha1_legacy_matches_the_fips_vectors() {
        assert_eq!(
            hex::encode(sha1_legacy("abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex::encode(sha1_legacy("")),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[quench_test::test]
    fn generic_digest_matches_the_fixed_forms() {
        assert_eq!(digest(Algorithm::Sha256, b"x"), sha256(b"x"));
        assert_eq!(digest(Algorithm::Sha384, b"x"), sha384(b"x"));
        assert_eq!(digest(Algorithm::Sha512, b"x"), sha512(b"x"));
        for (algorithm, len) in [
            (Algorithm::Sha256, 32),
            (Algorithm::Sha384, 48),
            (Algorithm::Sha512, 64),
        ] {
            assert_eq!(algorithm.output_len(), len);
            assert_eq!(digest(algorithm, b"").len(), len);
        }
    }

    #[quench_test::test]
    fn incremental_hashing_equals_one_shot() {
        let mut hasher = Hasher::new(Algorithm::Sha256);
        hasher.update("a");
        hasher.update(b"bc");
        let copy = hasher.clone();
        assert_eq!(hasher.finish(), sha256("abc"));
        assert_eq!(copy.finish(), sha256("abc"));
        assert!(format!("{:?}", Hasher::new(Algorithm::Sha512)).contains("Hasher"));
    }
}
