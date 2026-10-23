//! PBKDF2 (RFC 8018) with HMAC-SHA-2.

use std::num::NonZeroU32;

use ring::pbkdf2;

use crate::hmac::Algorithm;

const fn ring(algorithm: Algorithm) -> pbkdf2::Algorithm {
    match algorithm {
        Algorithm::Sha256 => pbkdf2::PBKDF2_HMAC_SHA256,
        Algorithm::Sha384 => pbkdf2::PBKDF2_HMAC_SHA384,
        Algorithm::Sha512 => pbkdf2::PBKDF2_HMAC_SHA512,
    }
}

/// Fills `out` with the derived key. `iterations` of 0 is treated as 1.
pub fn derive(algorithm: Algorithm, iterations: u32, salt: &[u8], password: &[u8], out: &mut [u8]) {
    let iterations = NonZeroU32::new(iterations).unwrap_or(NonZeroU32::MIN);
    pbkdf2::derive(ring(algorithm), iterations, salt, password, out);
}

/// Checks a derived key in constant time.
#[must_use]
pub fn verify(
    algorithm: Algorithm,
    iterations: u32,
    salt: &[u8],
    password: &[u8],
    expected: &[u8],
) -> bool {
    let iterations = NonZeroU32::new(iterations).unwrap_or(NonZeroU32::MIN);
    pbkdf2::verify(ring(algorithm), iterations, salt, password, expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // RFC 7914 section 11.
    #[test]
    fn rfc7914_vectors() {
        let mut out = [0u8; 64];
        derive(Algorithm::Sha256, 1, b"salt", b"passwd", &mut out);
        assert_eq!(
            hex::encode(out),
            "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
        );
        derive(Algorithm::Sha256, 80_000, b"NaCl", b"Password", &mut out);
        assert_eq!(
            hex::encode(out),
            "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56a1d425a1225833549adb841b51c9b3176a272bdebba1d078478f62b397f33c8d"
        );
    }

    #[test]
    fn verification_and_zero_iterations() {
        let mut key = [0u8; 32];
        derive(Algorithm::Sha512, 10, b"salt", b"pw", &mut key);
        assert!(verify(Algorithm::Sha512, 10, b"salt", b"pw", &key));
        assert!(!verify(Algorithm::Sha512, 10, b"salt", b"pX", &key));
        assert!(!verify(Algorithm::Sha512, 11, b"salt", b"pw", &key));
        let mut zero = [0u8; 16];
        let mut one = [0u8; 16];
        derive(Algorithm::Sha384, 0, b"s", b"p", &mut zero);
        derive(Algorithm::Sha384, 1, b"s", b"p", &mut one);
        assert_eq!(zero, one);
        assert!(verify(Algorithm::Sha384, 0, b"s", b"p", &one));
    }
}
