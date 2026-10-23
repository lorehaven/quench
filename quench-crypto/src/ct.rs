//! Constant-time comparison.

/// Whether `a` and `b` hold the same bytes.
///
/// Takes the same time whatever they contain: secrets, MACs and tokens are
/// compared with this, never with `==`. Slices of different lengths are
/// unequal; the length itself is not secret.
#[must_use]
pub fn eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Every byte is looked at; the differences are OR-ed together and only
    // inspected at the end, so no byte position changes what runs.
    let difference = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(difference) == 0
}

#[cfg(test)]
mod tests {
    use super::eq;

    #[test]
    fn compares_bytes() {
        assert!(eq(b"", b""));
        assert!(eq(b"secret", b"secret"));
        assert!(!eq(b"secret", b"secreT"));
        assert!(!eq(b"secret", b"secre"));
        assert!(!eq(b"", b"x"));
        // a difference in any position, first or last, is found
        for position in 0..8 {
            let mut other = *b"abcdefgh";
            other[position] ^= 0x80;
            assert!(!eq(b"abcdefgh", &other), "position {position}");
        }
    }
}
