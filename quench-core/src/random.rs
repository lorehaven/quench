//! Operating-system randomness.

use std::fmt;

/// The OS refused to provide entropy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RandomError;

impl fmt::Display for RandomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the operating system could not provide random bytes")
    }
}

impl std::error::Error for RandomError {}

/// Fills `buf` with cryptographically secure random bytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn try_fill(buf: &mut [u8]) -> Result<(), RandomError> {
    getrandom::fill(buf).map_err(|_| RandomError)
}

#[cfg(target_arch = "wasm32")]
static SOURCE: std::sync::OnceLock<fn(&mut [u8]) -> bool> = std::sync::OnceLock::new();

/// Installs the randomness source on `wasm32`.
///
/// `std` has none there; the host (the browser's `crypto.getRandomValues`)
/// supplies it. The function fills the buffer and returns whether it could.
/// The first call wins.
#[cfg(target_arch = "wasm32")]
pub fn set_source(source: fn(&mut [u8]) -> bool) {
    let _ = SOURCE.set(source);
}

/// Fills `buf` with cryptographically secure random bytes.
#[cfg(target_arch = "wasm32")]
pub fn try_fill(buf: &mut [u8]) -> Result<(), RandomError> {
    match SOURCE.get() {
        Some(source) if source(buf) => Ok(()),
        _ => Err(RandomError),
    }
}

/// Like [`try_fill`], for callers that cannot continue without entropy.
///
/// # Panics
///
/// Panics when the OS cannot provide randomness, which leaves nothing safe to
/// do: ids and tokens made from anything else would be guessable.
pub fn fill(buf: &mut [u8]) {
    try_fill(buf).expect("OS randomness unavailable");
}

/// `N` random bytes.
#[must_use]
pub fn bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    fill(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_with_entropy() {
        let a: [u8; 32] = bytes();
        let b: [u8; 32] = bytes();
        assert_ne!(a, b);
        assert_ne!(a, [0u8; 32]);
    }

    #[test]
    fn empty_buffer_is_fine() {
        assert!(try_fill(&mut []).is_ok());
    }

    #[test]
    fn error_displays() {
        assert!(RandomError.to_string().contains("random"));
    }
}
