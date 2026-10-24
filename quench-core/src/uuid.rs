//! RFC 4122 UUIDs. Only version 4 can be generated; any version can be parsed.

use std::fmt;
use std::str::FromStr;

use crate::random;

/// A 128-bit universally unique identifier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Uuid([u8; 16]);

/// The text was not a UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseUuidError(String);

impl fmt::Display for ParseUuidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid UUID: {}", self.0)
    }
}

impl std::error::Error for ParseUuidError {}

impl Uuid {
    /// The all-zero UUID.
    pub const NIL: Self = Self([0; 16]);

    /// A random (version 4, variant 1) UUID.
    #[must_use]
    pub fn new_v4() -> Self {
        let mut bytes: [u8; 16] = random::bytes();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self(bytes)
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    #[must_use]
    pub const fn is_nil(&self) -> bool {
        u128::from_be_bytes(self.0) == 0
    }

    /// The version nibble (4 for anything [`new_v4`](Self::new_v4) made).
    #[must_use]
    pub const fn version(&self) -> u8 {
        self.0[6] >> 4
    }

    /// Formats without hyphens: 32 lowercase hex digits.
    #[must_use]
    pub const fn simple(&self) -> Simple {
        Simple(*self)
    }

    /// Parses the hyphenated (8-4-4-4-12) or simple (32 digits) form, in any case.
    pub fn parse(text: &str) -> Result<Self, ParseUuidError> {
        let bad = || ParseUuidError(text.to_string());
        let digits: Vec<u8> = match text.len() {
            36 => {
                let bytes = text.as_bytes();
                if [8, 13, 18, 23].iter().any(|&i| bytes[i] != b'-') {
                    return Err(bad());
                }
                bytes.iter().copied().filter(|&b| b != b'-').collect()
            }
            32 => text.bytes().collect(),
            _ => return Err(bad()),
        };
        if digits.len() != 32 {
            return Err(bad());
        }
        let mut out = [0u8; 16];
        for (slot, pair) in out.iter_mut().zip(digits.chunks(2)) {
            let hi = nibble(pair[0]).ok_or_else(bad)?;
            let lo = nibble(pair[1]).ok_or_else(bad)?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
    }
}

const fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

impl FromStr for Uuid {
    type Err = ParseUuidError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, byte) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid({self})")
    }
}

/// A [`Uuid`] displayed without hyphens.
#[derive(Debug, Clone, Copy)]
pub struct Simple(Uuid);

impl fmt::Display for Simple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", u128::from_be_bytes(self.0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn v4_has_version_and_variant_bits() {
        for _ in 0..64 {
            let id = Uuid::new_v4();
            assert_eq!(id.version(), 4);
            assert_eq!(id.as_bytes()[8] & 0xc0, 0x80);
            assert!(!id.is_nil());
        }
    }

    #[quench_test::test]
    fn v4_values_differ() {
        assert_ne!(Uuid::new_v4(), Uuid::new_v4());
    }

    #[quench_test::test]
    fn displays_hyphenated_and_simple() {
        let id = Uuid::from_bytes([
            0x12, 0x3e, 0x45, 0x67, 0xe8, 0x9b, 0x12, 0xd3, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17,
            0x40, 0x00,
        ]);
        assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
        assert_eq!(id.simple().to_string(), "123e4567e89b12d3a456426614174000");
        assert_eq!(
            format!("{id:?}"),
            "Uuid(123e4567-e89b-12d3-a456-426614174000)"
        );
        assert_eq!(id.version(), 1);
    }

    #[quench_test::test]
    fn nil_is_all_zero() {
        assert_eq!(
            Uuid::NIL.to_string(),
            "00000000-0000-0000-0000-000000000000"
        );
        assert!(Uuid::NIL.is_nil());
        assert_eq!(Uuid::default(), Uuid::NIL);
    }

    #[quench_test::test]
    fn parses_both_forms_in_any_case() {
        let want = Uuid::parse("123e4567-e89b-12d3-a456-426614174000").unwrap();
        assert_eq!(
            Uuid::parse("123E4567-E89B-12D3-A456-426614174000").unwrap(),
            want
        );
        assert_eq!(
            Uuid::parse("123e4567e89b12d3a456426614174000").unwrap(),
            want
        );
        assert_eq!(
            "123e4567e89b12d3a456426614174000".parse::<Uuid>().unwrap(),
            want
        );
    }

    #[quench_test::test]
    fn round_trips_random_ids() {
        for _ in 0..64 {
            let id = Uuid::new_v4();
            assert_eq!(Uuid::parse(&id.to_string()).unwrap(), id);
            assert_eq!(Uuid::parse(&id.simple().to_string()).unwrap(), id);
        }
    }

    #[quench_test::test]
    fn rejects_malformed_text() {
        for bad in [
            "",
            "123e4567",
            "123e4567-e89b-12d3-a456-42661417400",
            "123e4567-e89b-12d3-a456-4266141740000",
            "123e4567xe89b-12d3-a456-426614174000",
            "123e4567-e89b-12d3-a456-42661417400g",
            "g23e4567e89b12d3a456426614174000",
            "123e4567e89b12d3a45642661417400",
            // 36 long, hyphens in the wrong places
            "123e4567e-89b-12d3-a456-426614174000",
        ] {
            let err = Uuid::parse(bad).unwrap_err();
            assert!(err.to_string().contains("invalid UUID"), "{bad}");
        }
    }

    #[quench_test::test]
    fn orders_bytewise() {
        let low = Uuid::from_bytes([0; 16]);
        let high = Uuid::from_bytes([1; 16]);
        assert!(low < high);
    }
}
