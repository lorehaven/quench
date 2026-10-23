//! HKDF (RFC 5869) with HMAC-SHA-256.

use ring::hkdf::{HKDF_SHA256, Salt};

/// Output length too large for HKDF-SHA-256 (more than 255 hash blocks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLong;

impl std::fmt::Display for TooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HKDF output length is more than 255 * 32 bytes")
    }
}

impl std::error::Error for TooLong {}

struct Len(usize);

impl ring::hkdf::KeyType for Len {
    fn len(&self) -> usize {
        self.0
    }
}

/// `length` bytes derived from the input key material `ikm` with `salt` (may be empty) and context `info`.
///
/// # Errors
///
/// [`TooLong`] when `length` is above 8160.
pub fn sha256(ikm: &[u8], salt: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, TooLong> {
    let prk = Salt::new(HKDF_SHA256, salt).extract(ikm);
    let info = [info];
    let okm = prk.expand(&info, Len(length)).map_err(|_| TooLong)?;
    let mut out = vec![0u8; length];
    okm.fill(&mut out).map_err(|_| TooLong)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len() / 2)
            .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn rfc_5869_test_cases() {
        // Test case 1.
        let okm = sha256(
            &hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
            &hex("000102030405060708090a0b0c"),
            &hex("f0f1f2f3f4f5f6f7f8f9"),
            42,
        )
        .unwrap();
        assert_eq!(
            okm,
            hex(
                "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
            )
        );
        // Test case 3: empty salt and info.
        let okm = sha256(
            &hex("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"),
            &[],
            &[],
            42,
        )
        .unwrap();
        assert_eq!(
            okm,
            hex(
                "8da4e775a563c18f715f802a063c5a31b8a11f5c5ee1879ec3454e5f3c738d2d9d201395faa4b61a96c8"
            )
        );
        assert_eq!(sha256(b"k", b"", b"", 0).unwrap(), Vec::<u8>::new());
        assert_eq!(sha256(b"k", b"", b"", 8161), Err(TooLong));
        assert!(TooLong.to_string().contains("255"));
    }
}
