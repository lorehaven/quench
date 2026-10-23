//! HMAC with SHA-2 (RFC 2104).

use ring::hmac;

/// The hash an HMAC is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// Only for protocols that fix it (TOTP, WebSocket-era tokens); new designs use SHA-256 or better.
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    const fn ring(self) -> hmac::Algorithm {
        match self {
            Self::Sha1 => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            Self::Sha256 => hmac::HMAC_SHA256,
            Self::Sha384 => hmac::HMAC_SHA384,
            Self::Sha512 => hmac::HMAC_SHA512,
        }
    }
}

/// The authentication tag of `data` under `key`.
#[must_use]
pub fn sign(algorithm: Algorithm, key: &[u8], data: impl AsRef<[u8]>) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(algorithm.ring(), key), data.as_ref())
        .as_ref()
        .to_vec()
}

/// Checks `tag` against `data` in constant time.
#[must_use]
pub fn verify(algorithm: Algorithm, key: &[u8], data: impl AsRef<[u8]>, tag: &[u8]) -> bool {
    hmac::verify(&hmac::Key::new(algorithm.ring(), key), data.as_ref(), tag).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // RFC 4231 test cases 1 and 2.
    #[test]
    fn rfc4231_vectors() {
        let key = [0x0b; 20];
        assert_eq!(
            hex::encode(sign(Algorithm::Sha256, &key, "Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex::encode(sign(Algorithm::Sha384, &key, "Hi There")),
            "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59cfaea9ea9076ede7f4af152e8b2fa9cb6"
        );
        assert_eq!(
            hex::encode(sign(Algorithm::Sha512, &key, "Hi There")),
            "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854"
        );
        assert_eq!(
            hex::encode(sign(
                Algorithm::Sha256,
                b"Jefe",
                "what do ya want for nothing?"
            )),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn verification() {
        let tag = sign(Algorithm::Sha256, b"k", "message");
        assert!(verify(Algorithm::Sha256, b"k", "message", &tag));
        assert!(!verify(Algorithm::Sha256, b"k", "messagf", &tag));
        assert!(!verify(Algorithm::Sha256, b"other", "message", &tag));
        assert!(!verify(Algorithm::Sha256, b"k", "message", &tag[1..]));
        assert!(!verify(Algorithm::Sha512, b"k", "message", &tag));
    }
}
