//! Lower- and upper-case hexadecimal.

use std::fmt;

/// Why a string did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// An odd number of digits.
    OddLength,
    /// A byte that is not a hex digit, at this offset.
    InvalidByte(usize),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OddLength => f.write_str("hex string has an odd length"),
            Self::InvalidByte(at) => write!(f, "invalid hex digit at offset {at}"),
        }
    }
}

impl std::error::Error for DecodeError {}

const LOWER: &[u8; 16] = b"0123456789abcdef";
const UPPER: &[u8; 16] = b"0123456789ABCDEF";

fn encode_with(input: &[u8], digits: &[u8; 16]) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for &byte in input {
        out.push(char::from(digits[usize::from(byte >> 4)]));
        out.push(char::from(digits[usize::from(byte & 0x0f)]));
    }
    out
}

#[must_use]
pub fn encode(input: impl AsRef<[u8]>) -> String {
    encode_with(input.as_ref(), LOWER)
}

#[must_use]
pub fn encode_upper(input: impl AsRef<[u8]>) -> String {
    encode_with(input.as_ref(), UPPER)
}

/// Decodes digits of either case.
pub fn decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError> {
    let input = input.as_ref();
    if input.len() % 2 != 0 {
        return Err(DecodeError::OddLength);
    }
    let nibble = |at: usize| match input[at] {
        d @ b'0'..=b'9' => Ok(d - b'0'),
        d @ b'a'..=b'f' => Ok(d - b'a' + 10),
        d @ b'A'..=b'F' => Ok(d - b'A' + 10),
        _ => Err(DecodeError::InvalidByte(at)),
    };
    (0..input.len() / 2)
        .map(|i| Ok(nibble(2 * i)? << 4 | nibble(2 * i + 1)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn encodes_both_cases() {
        assert_eq!(encode([0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(encode_upper([0x00, 0x0f, 0xa5, 0xff]), "000FA5FF");
        assert_eq!(encode(""), "");
    }

    #[quench_test::test]
    fn decodes_either_case() {
        assert_eq!(decode("000fA5Ff").unwrap(), [0x00, 0x0f, 0xa5, 0xff]);
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[quench_test::test]
    fn round_trips_every_byte() {
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode(encode(&all)).unwrap(), all);
        assert_eq!(decode(encode_upper(&all)).unwrap(), all);
    }

    #[quench_test::test]
    fn rejects_bad_input() {
        assert_eq!(decode("abc"), Err(DecodeError::OddLength));
        assert_eq!(decode("0g"), Err(DecodeError::InvalidByte(1)));
        assert_eq!(decode("zz"), Err(DecodeError::InvalidByte(0)));
        assert_eq!(decode("0 "), Err(DecodeError::InvalidByte(1)));
        assert!(DecodeError::OddLength.to_string().contains("odd"));
        assert!(DecodeError::InvalidByte(1).to_string().contains("offset 1"));
    }
}
