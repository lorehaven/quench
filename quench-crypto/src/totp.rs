//! One-time passwords: HOTP (RFC 4226) and TOTP (RFC 6238), as authenticator apps use them.

use crate::{ct, hmac};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// The shortest secret RFC 4226 allows (128 bits).
pub const MIN_SECRET_BYTES: usize = 16;

/// The HMAC a code is built on. Apps default to SHA-1; not every app honours the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    const fn hmac(self) -> hmac::Algorithm {
        match self {
            Self::Sha1 => hmac::Algorithm::Sha1,
            Self::Sha256 => hmac::Algorithm::Sha256,
            Self::Sha512 => hmac::Algorithm::Sha512,
        }
    }

    /// The name an `otpauth://` URL uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }
}

/// Why a [`Totp`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Fewer than [`MIN_SECRET_BYTES`] bytes.
    SecretTooShort(usize),
    /// Not valid base32.
    BadBase32(quench_core::base32::DecodeError),
    /// Digits must be 6 to 9.
    BadDigits(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecretTooShort(len) => {
                write!(f, "a one-time-password secret needs at least {MIN_SECRET_BYTES} bytes, got {len}")
            }
            Self::BadBase32(why) => write!(f, "the secret is not valid base32: {why}"),
            Self::BadDigits(digits) => write!(f, "a code has 6 to 9 digits, not {digits}"),
        }
    }
}

impl std::error::Error for Error {}

/// A fresh random secret of 20 bytes (160 bits, what SHA-1 HMAC wants).
#[must_use]
pub fn generate_secret() -> Vec<u8> {
    quench_core::random::bytes::<20>().to_vec()
}

/// The HOTP value of `counter` under `key`: `digits` decimal digits, zero-padded.
#[must_use]
pub fn hotp(algorithm: Algorithm, key: &[u8], counter: u64, digits: u32) -> String {
    let tag = hmac::sign(algorithm.hmac(), key, counter.to_be_bytes());
    // Dynamic truncation (RFC 4226 section 5.3).
    let offset = usize::from(tag[tag.len() - 1] & 0x0f);
    let word = u32::from_be_bytes([tag[offset] & 0x7f, tag[offset + 1], tag[offset + 2], tag[offset + 3]]);
    let modulus = 10u32.pow(digits.min(9));
    format!("{:0width$}", word % modulus, width = digits.min(9) as usize)
}

/// A time-based one-time-password generator and checker.
#[derive(Clone, PartialEq, Eq)]
pub struct Totp {
    secret: Vec<u8>,
    algorithm: Algorithm,
    digits: u32,
    step: u64,
    skew: u64,
}

// Hand-written: the secret must never end up in a log line.
impl fmt::Debug for Totp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Totp")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("step", &self.step)
            .field("skew", &self.skew)
            .finish_non_exhaustive()
    }
}

impl Totp {
    /// SHA-1, 6 digits, a 30-second step and one step of skew either way: what every authenticator app does.
    ///
    /// # Errors
    ///
    /// [`Error::SecretTooShort`] under 16 bytes.
    pub fn new(secret: Vec<u8>) -> Result<Self, Error> {
        if secret.len() < MIN_SECRET_BYTES {
            return Err(Error::SecretTooShort(secret.len()));
        }
        Ok(Self { secret, algorithm: Algorithm::Sha1, digits: 6, step: 30, skew: 1 })
    }

    /// Like [`new`](Self::new) from the base32 text apps display and accept.
    ///
    /// # Errors
    ///
    /// [`Error::BadBase32`] or [`Error::SecretTooShort`].
    pub fn from_base32(secret: &str) -> Result<Self, Error> {
        Self::new(quench_core::base32::decode(secret).map_err(Error::BadBase32)?)
    }

    #[must_use]
    pub const fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// # Errors
    ///
    /// [`Error::BadDigits`] outside 6 to 9.
    pub fn with_digits(mut self, digits: u32) -> Result<Self, Error> {
        if !(6..=9).contains(&digits) {
            return Err(Error::BadDigits(digits));
        }
        self.digits = digits;
        Ok(self)
    }

    /// Seconds per code (at least 1).
    #[must_use]
    pub const fn with_step(mut self, seconds: u64) -> Self {
        self.step = if seconds == 0 { 1 } else { seconds };
        self
    }

    /// How many steps before and after the current one a code is still accepted.
    #[must_use]
    pub const fn with_skew(mut self, steps: u64) -> Self {
        self.skew = steps;
        self
    }

    /// The secret as base32 text, for manual entry.
    #[must_use]
    pub fn secret_base32(&self) -> String {
        quench_core::base32::encode(&self.secret)
    }

    /// The code valid at `unix_seconds`.
    #[must_use]
    pub fn code_at(&self, unix_seconds: u64) -> String {
        hotp(self.algorithm, &self.secret, unix_seconds / self.step, self.digits)
    }

    /// The code valid now.
    #[must_use]
    pub fn code_now(&self) -> String {
        self.code_at(now())
    }

    /// Whether `code` is valid at `unix_seconds`, within the skew. Compared in constant time.
    #[must_use]
    pub fn check_at(&self, code: &str, unix_seconds: u64) -> bool {
        let counter = unix_seconds / self.step;
        let code = code.trim();
        let mut accepted = false;
        // Every window is computed and compared: how many matched early must not show in the timing.
        for window in counter.saturating_sub(self.skew)..=counter.saturating_add(self.skew) {
            let expected = hotp(self.algorithm, &self.secret, window, self.digits);
            accepted |= ct::eq(expected.as_bytes(), code.as_bytes());
        }
        accepted
    }

    /// Whether `code` is valid now.
    #[must_use]
    pub fn check_now(&self, code: &str) -> bool {
        self.check_at(code, now())
    }

    /// The `otpauth://totp/...` URL an authenticator app imports (usually shown as a QR code).
    #[must_use]
    pub fn url(&self, issuer: &str, account: &str) -> String {
        let issuer_text = percent(issuer);
        format!(
            "otpauth://totp/{issuer_text}:{}?secret={}&issuer={issuer_text}&algorithm={}&digits={}&period={}",
            percent(account),
            self.secret_base32(),
            self.algorithm.name(),
            self.digits,
            self.step,
        )
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Percent-encodes everything but the RFC 3986 unreserved characters.
fn percent(text: &str) -> String {
    use fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4226 appendix D: key "12345678901234567890".
    #[test]
    fn hotp_matches_the_rfc_4226_vectors() {
        let expected = ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"];
        for (counter, code) in expected.iter().enumerate() {
            assert_eq!(hotp(Algorithm::Sha1, b"12345678901234567890", counter as u64, 6), *code);
        }
    }

    // RFC 6238 appendix B, 8 digits, per-algorithm keys.
    #[test]
    fn totp_matches_the_rfc_6238_vectors() {
        let sha1 = Totp::new(b"12345678901234567890".to_vec()).unwrap().with_digits(8).unwrap();
        let sha256 = Totp::new(b"12345678901234567890123456789012".to_vec()).unwrap().with_algorithm(Algorithm::Sha256).with_digits(8).unwrap();
        let sha512 = Totp::new(b"1234567890123456789012345678901234567890123456789012345678901234".to_vec())
            .unwrap()
            .with_algorithm(Algorithm::Sha512)
            .with_digits(8)
            .unwrap();
        for (time, c1, c256, c512) in [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ] {
            assert_eq!(sha1.code_at(time), c1, "sha1 at {time}");
            assert_eq!(sha256.code_at(time), c256, "sha256 at {time}");
            assert_eq!(sha512.code_at(time), c512, "sha512 at {time}");
        }
    }

    #[test]
    fn checking_allows_the_skew_and_nothing_more() {
        let totp = Totp::new(generate_secret()).unwrap();
        let t = 1_700_000_000;
        assert!(totp.check_at(&totp.code_at(t), t));
        assert!(totp.check_at(&totp.code_at(t - 30), t), "one step old");
        assert!(totp.check_at(&totp.code_at(t + 30), t), "one step early");
        assert!(!totp.check_at(&totp.code_at(t - 120), t));
        assert!(!totp.check_at("000000", t) || totp.code_at(t) == "000000");
        assert!(totp.check_at(&format!(" {} ", totp.code_at(t)), t), "surrounding space is ignored");
        assert!(!totp.check_at("12345", t) && !totp.check_at("", t));
        let strict = totp.clone().with_skew(0);
        assert!(!strict.check_at(&strict.code_at(t - 30), t));
        assert!(totp.check_now(&totp.code_now()));
    }

    #[test]
    fn construction_validates_and_the_secret_stays_out_of_debug() {
        assert_eq!(Totp::new(vec![1; 15]), Err(Error::SecretTooShort(15)));
        assert!(Totp::from_base32("not base32!").is_err());
        assert_eq!(Totp::new(vec![1; 16]).unwrap().with_digits(5), Err(Error::BadDigits(5)));
        assert_eq!(Totp::new(vec![1; 16]).unwrap().with_digits(10), Err(Error::BadDigits(10)));
        let totp = Totp::new(b"12345678901234567890".to_vec()).unwrap();
        let shown = format!("{totp:?}");
        assert!(!shown.contains("GEZDG") && !shown.contains("12345"), "{shown}");
        assert_eq!(Totp::from_base32(&totp.secret_base32()).unwrap(), totp);
        assert_eq!(totp.clone().with_step(0).code_at(5), totp.with_step(1).code_at(5));
        for error in [Error::SecretTooShort(3), Error::BadBase32(quench_core::base32::DecodeError::InvalidLength), Error::BadDigits(4)] {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn the_url_is_what_apps_import() {
        let totp = Totp::new(b"12345678901234567890".to_vec()).unwrap();
        assert_eq!(
            totp.url("Forge Co", "ann@example.com"),
            "otpauth://totp/Forge%20Co:ann%40example.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Forge%20Co&algorithm=SHA1&digits=6&period=30"
        );
        assert_ne!(generate_secret(), generate_secret());
        assert_eq!(generate_secret().len(), 20);
        for algorithm in [Algorithm::Sha1, Algorithm::Sha256, Algorithm::Sha512] {
            assert!(algorithm.name().starts_with("SHA"));
        }
    }
}
