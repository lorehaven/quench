//! RFC 4648 base64: the standard and URL-safe alphabets, padded or not.
//!
//! Decoding is strict, like the `base64` crate's defaults: characters outside
//! the alphabet, misplaced or missing padding, and non-zero trailing bits are
//! all errors. Callers verify signatures and tokens over what they decode, so
//! two spellings of one value would be a bug, not a convenience.

use std::fmt;

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_SAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// An alphabet plus a padding rule.
#[derive(Debug, Clone, Copy)]
pub struct Base64 {
    alphabet: &'static [u8; 64],
    padded: bool,
}

/// `+` and `/`, with `=` padding.
pub const STANDARD: Base64 = Base64 {
    alphabet: STANDARD_ALPHABET,
    padded: true,
};
/// `+` and `/`, no padding.
pub const STANDARD_NO_PAD: Base64 = Base64 {
    alphabet: STANDARD_ALPHABET,
    padded: false,
};
/// `-` and `_`, with `=` padding.
pub const URL_SAFE: Base64 = Base64 {
    alphabet: URL_SAFE_ALPHABET,
    padded: true,
};
/// `-` and `_`, no padding (JWT and friends).
pub const URL_SAFE_NO_PAD: Base64 = Base64 {
    alphabet: URL_SAFE_ALPHABET,
    padded: false,
};

/// Why a string did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// A byte that is not in the alphabet, at this offset.
    InvalidByte(usize),
    /// The length cannot come from an encoder (one dangling character).
    InvalidLength,
    /// Padding that is missing, extra, or not at the end.
    InvalidPadding,
    /// The last character carries bits that an encoder would have left zero.
    NonCanonical,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByte(at) => write!(f, "invalid base64 byte at offset {at}"),
            Self::InvalidLength => f.write_str("invalid base64 length"),
            Self::InvalidPadding => f.write_str("invalid base64 padding"),
            Self::NonCanonical => f.write_str("non-canonical base64 trailing bits"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl Base64 {
    #[must_use]
    pub fn encode(&self, input: impl AsRef<[u8]>) -> String {
        let input = input.as_ref();
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
            let sextets = chunk.len() + 1;
            for i in 0..4 {
                if i < sextets {
                    out.push(char::from(
                        self.alphabet[(n >> (18 - 6 * i) & 0x3f) as usize],
                    ));
                } else if self.padded {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(&self, input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError> {
        let input = input.as_ref();
        let pad = input.iter().rev().take_while(|&&b| b == b'=').count();
        let body = &input[..input.len() - pad];
        let wanted_pad = match body.len() % 4 {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return Err(DecodeError::InvalidLength),
        };
        // Unpadded input must have none; padded input must be padded exactly
        // up to a multiple of four.
        if pad != if self.padded { wanted_pad } else { 0 } {
            return Err(DecodeError::InvalidPadding);
        }

        let mut out = Vec::with_capacity(body.len() / 4 * 3 + 2);
        let mut acc = 0u32;
        let mut bits = 0u32;
        for (at, &byte) in body.iter().enumerate() {
            let value = self.value(byte).ok_or(DecodeError::InvalidByte(at))?;
            acc = acc << 6 | u32::from(value);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                // `acc` holds at most 14 bits here, so the top byte fits.
                #[allow(clippy::cast_possible_truncation)]
                out.push((acc >> bits) as u8);
                acc &= (1 << bits) - 1;
            }
        }
        if acc != 0 {
            return Err(DecodeError::NonCanonical);
        }
        Ok(out)
    }

    fn value(&self, byte: u8) -> Option<u8> {
        self.alphabet
            .iter()
            .position(|&c| c == byte)
            .and_then(|p| u8::try_from(p).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4648 section 10.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ];

    #[quench_test::test]
    fn rfc4648_vectors() {
        for (plain, encoded) in VECTORS {
            assert_eq!(STANDARD.encode(plain), encoded);
            assert_eq!(STANDARD.decode(encoded).unwrap(), plain.as_bytes());
            let bare = encoded.trim_end_matches('=');
            assert_eq!(STANDARD_NO_PAD.encode(plain), bare);
            assert_eq!(STANDARD_NO_PAD.decode(bare).unwrap(), plain.as_bytes());
            assert_eq!(URL_SAFE.encode(plain), encoded);
            assert_eq!(URL_SAFE_NO_PAD.encode(plain), bare);
        }
    }

    #[quench_test::test]
    fn alphabets_differ_on_62_and_63() {
        let bytes = [0xfb, 0xff, 0xbf];
        assert_eq!(STANDARD.encode(bytes), "+/+/");
        assert_eq!(URL_SAFE.encode(bytes), "-_-_");
        assert_eq!(URL_SAFE.decode("-_-_").unwrap(), bytes);
        assert_eq!(STANDARD.decode("-_-_"), Err(DecodeError::InvalidByte(0)));
        assert_eq!(URL_SAFE.decode("+/+/"), Err(DecodeError::InvalidByte(0)));
    }

    #[quench_test::test]
    fn round_trips_every_length_and_byte() {
        let all: Vec<u8> = (0..=255).collect();
        for len in 0..=all.len() {
            for engine in [STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD] {
                let text = engine.encode(&all[..len]);
                assert_eq!(engine.decode(&text).unwrap(), &all[..len], "len {len}");
            }
        }
    }

    #[quench_test::test]
    fn rejects_bad_alphabet_bytes() {
        assert_eq!(STANDARD.decode("Zm9*"), Err(DecodeError::InvalidByte(3)));
        assert_eq!(STANDARD.decode("Zm 9"), Err(DecodeError::InvalidByte(2)));
        assert_eq!(STANDARD.decode("Zm9\n"), Err(DecodeError::InvalidByte(3)));
        assert_eq!(
            STANDARD.decode([b'Z', 0xff, b'9', b'v']),
            Err(DecodeError::InvalidByte(1))
        );
    }

    #[quench_test::test]
    fn rejects_bad_padding() {
        for bad in [
            "Zg=", "Zg", "Zg===", "Z===", "====", "Zm9v=", "=Zg=", "Zg=a", "Zm=9",
        ] {
            assert!(STANDARD.decode(bad).is_err(), "{bad}");
        }
        assert_eq!(
            STANDARD_NO_PAD.decode("Zg=="),
            Err(DecodeError::InvalidPadding)
        );
        assert_eq!(
            URL_SAFE_NO_PAD.decode("Zm8="),
            Err(DecodeError::InvalidPadding)
        );
    }

    #[quench_test::test]
    fn rejects_impossible_lengths() {
        assert_eq!(STANDARD_NO_PAD.decode("Z"), Err(DecodeError::InvalidLength));
        assert_eq!(
            STANDARD_NO_PAD.decode("Zm9vY"),
            Err(DecodeError::InvalidLength)
        );
        assert_eq!(STANDARD.decode("Z==="), Err(DecodeError::InvalidLength));
        assert_eq!(STANDARD.decode("Zm9vY"), Err(DecodeError::InvalidLength));
    }

    #[quench_test::test]
    fn rejects_non_zero_trailing_bits() {
        // "Zh==" would decode to 'f' with stray low bits set.
        assert_eq!(STANDARD.decode("Zh=="), Err(DecodeError::NonCanonical));
        assert_eq!(STANDARD.decode("Zm9="), Err(DecodeError::NonCanonical));
        assert_eq!(URL_SAFE_NO_PAD.decode("Zh"), Err(DecodeError::NonCanonical));
    }

    #[quench_test::test]
    fn errors_display() {
        for e in [
            DecodeError::InvalidByte(3),
            DecodeError::InvalidLength,
            DecodeError::InvalidPadding,
            DecodeError::NonCanonical,
        ] {
            assert!(e.to_string().contains("base64"));
        }
    }
}
