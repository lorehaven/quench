//! Base32 (RFC 4648, the `A-Z2-7` alphabet): what authenticator apps show for a TOTP secret.

use std::fmt;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Why a string did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// A character outside the alphabet, at this offset.
    InvalidByte(usize),
    /// The trailing group cannot come from whole bytes (1, 3 or 6 leftover characters).
    InvalidLength,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByte(at) => write!(f, "invalid base32 character at offset {at}"),
            Self::InvalidLength => f.write_str("base32 string has an impossible length"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Encodes without padding, in upper case.
#[must_use]
pub fn encode(input: impl AsRef<[u8]>) -> String {
    let mut out = String::with_capacity(input.as_ref().len().div_ceil(5) * 8);
    let mut buffer = 0u16;
    let mut bits = 0u8;
    for &byte in input.as_ref() {
        buffer = (buffer << 8) | u16::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[usize::from((buffer >> bits) & 0x1f)]));
        }
    }
    if bits > 0 {
        out.push(char::from(
            ALPHABET[usize::from((buffer << (5 - bits)) & 0x1f)],
        ));
    }
    out
}

/// Decodes, ignoring case, `=` padding, spaces and hyphens (how a secret is usually typed in groups).
///
/// # Errors
///
/// [`DecodeError`] for a character outside the alphabet or an impossible length.
pub fn decode(input: &str) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(input.len() * 5 / 8);
    let mut buffer = 0u16;
    let mut bits = 0u8;
    let mut symbols = 0usize;
    for (at, ch) in input.bytes().enumerate() {
        let value = match ch {
            b'A'..=b'Z' => ch - b'A',
            b'a'..=b'z' => ch - b'a',
            b'2'..=b'7' => ch - b'2' + 26,
            b'=' | b' ' | b'-' => continue,
            _ => return Err(DecodeError::InvalidByte(at)),
        };
        symbols += 1;
        buffer = (buffer << 5) | u16::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).unwrap_or(0));
        }
    }
    if matches!(symbols % 8, 1 | 3 | 6) {
        return Err(DecodeError::InvalidLength);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4648 section 10.
    const VECTORS: [(&str, &str); 7] = [
        ("", ""),
        ("f", "MY"),
        ("fo", "MZXQ"),
        ("foo", "MZXW6"),
        ("foob", "MZXW6YQ"),
        ("fooba", "MZXW6YTB"),
        ("foobar", "MZXW6YTBOI"),
    ];

    #[test]
    fn rfc_4648_vectors_round_trip() {
        for (plain, encoded) in VECTORS {
            assert_eq!(encode(plain), encoded);
            assert_eq!(decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn decoding_is_forgiving_about_case_padding_and_grouping() {
        assert_eq!(decode("mzxw 6ytb-oi======").unwrap(), b"foobar");
        assert_eq!(decode("MZXW6YQ=").unwrap(), b"foob");
    }

    #[test]
    fn bad_input_is_an_error() {
        assert_eq!(decode("MZ1W"), Err(DecodeError::InvalidByte(2)));
        assert_eq!(decode("M"), Err(DecodeError::InvalidLength));
        assert_eq!(decode("MZX"), Err(DecodeError::InvalidLength));
        assert!(DecodeError::InvalidByte(2).to_string().contains("offset 2"));
        assert!(DecodeError::InvalidLength.to_string().contains("length"));
    }

    #[test]
    fn every_length_of_random_looking_bytes_round_trips() {
        for len in 0..40u8 {
            let data: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(11))
                .collect();
            assert_eq!(decode(&encode(&data)).unwrap(), data);
        }
    }
}
