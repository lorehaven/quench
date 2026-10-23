//! Authenticated encryption with ChaCha20-Poly1305 (RFC 8439) and XChaCha20-Poly1305.
//!
//! Both seal `plaintext` under a 32-byte key and bind optional associated data (`aad`): data that is not
//! secret but must match when the value is opened, so a ciphertext copied to another row or context fails.
//! The `seal`/`open` pair generates a random nonce and keeps it in front of the ciphertext, which is what
//! storage usually wants: `nonce || ciphertext || tag`.
//!
//! ChaCha20-Poly1305 has a 12-byte nonce, which is too short to pick at random for a long-lived key;
//! XChaCha20-Poly1305 has a 24-byte one that is.

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use std::fmt;

/// Key length of both ciphers.
pub const KEY_LEN: usize = 32;
/// Nonce length of [`ChaCha20Poly1305`].
pub const NONCE_LEN: usize = 12;
/// Nonce length of [`XChaCha20Poly1305`].
pub const XNONCE_LEN: usize = 24;
/// The authentication tag appended to every ciphertext.
pub const TAG_LEN: usize = 16;

/// Opening failed: wrong key, wrong associated data, a truncated or altered ciphertext. Deliberately says
/// nothing about which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenError;

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "decryption failed: wrong key or associated data, or the ciphertext was altered",
        )
    }
}

impl std::error::Error for OpenError {}

fn key(bytes: &[u8; KEY_LEN]) -> LessSafeKey {
    LessSafeKey::new(
        UnboundKey::new(&CHACHA20_POLY1305, bytes)
            .expect("a 32-byte key is valid for ChaCha20-Poly1305"),
    )
}

fn encrypt(key: &LessSafeKey, nonce: [u8; NONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let mut buffer = plaintext.to_vec();
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        &mut buffer,
    )
    .expect("a ChaCha20-Poly1305 seal cannot fail for a message that fits in memory");
    buffer
}

fn decrypt(
    key: &LessSafeKey,
    nonce: [u8; NONCE_LEN],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, OpenError> {
    let mut buffer = ciphertext.to_vec();
    let plain = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut buffer,
        )
        .map_err(|_| OpenError)?;
    let len = plain.len();
    buffer.truncate(len);
    Ok(buffer)
}

/// ChaCha20-Poly1305 with a 12-byte nonce.
#[derive(Clone)]
pub struct ChaCha20Poly1305 {
    key: [u8; KEY_LEN],
}

// Hand-written: the key must never end up in a log line.
impl fmt::Debug for ChaCha20Poly1305 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChaCha20Poly1305 { .. }")
    }
}

impl ChaCha20Poly1305 {
    #[must_use]
    pub const fn new(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }

    /// `ciphertext || tag` for `plaintext` under `nonce`. A nonce must never be used twice with one key.
    #[must_use]
    pub fn encrypt(&self, nonce: [u8; NONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
        encrypt(&key(&self.key), nonce, aad, plaintext)
    }

    /// # Errors
    ///
    /// [`OpenError`] when the tag does not verify.
    pub fn decrypt(
        &self,
        nonce: [u8; NONCE_LEN],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, OpenError> {
        decrypt(&key(&self.key), nonce, aad, ciphertext)
    }

    /// `nonce || ciphertext || tag` under a fresh random nonce.
    #[must_use]
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let nonce = quench_core::random::bytes::<NONCE_LEN>();
        let mut out = nonce.to_vec();
        out.extend(self.encrypt(nonce, aad, plaintext));
        out
    }

    /// Opens what [`seal`](Self::seal) produced.
    ///
    /// # Errors
    ///
    /// [`OpenError`] when it is too short or the tag does not verify.
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, OpenError> {
        let (nonce, ciphertext) = sealed.split_at_checked(NONCE_LEN).ok_or(OpenError)?;
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| OpenError)?;
        self.decrypt(nonce, aad, ciphertext)
    }
}

/// XChaCha20-Poly1305: a 24-byte nonce, safe to choose at random.
#[derive(Clone)]
pub struct XChaCha20Poly1305 {
    key: [u8; KEY_LEN],
}

impl fmt::Debug for XChaCha20Poly1305 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("XChaCha20Poly1305 { .. }")
    }
}

impl XChaCha20Poly1305 {
    #[must_use]
    pub const fn new(key: [u8; KEY_LEN]) -> Self {
        Self { key }
    }

    /// The ChaCha20-Poly1305 key and nonce an extended nonce stands for: the key mixed with the first 16
    /// nonce bytes, and the last 8 behind four zero bytes.
    fn derive(&self, nonce: &[u8; XNONCE_LEN]) -> (LessSafeKey, [u8; NONCE_LEN]) {
        let (head, tail) = nonce.split_at(16);
        let mut inner = [0u8; NONCE_LEN];
        inner[4..].copy_from_slice(tail);
        (key(&hchacha20(&self.key, head)), inner)
    }

    /// `ciphertext || tag` for `plaintext` under `nonce`.
    #[must_use]
    pub fn encrypt(&self, nonce: &[u8; XNONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let (key, inner) = self.derive(nonce);
        encrypt(&key, inner, aad, plaintext)
    }

    /// # Errors
    ///
    /// [`OpenError`] when the tag does not verify.
    pub fn decrypt(
        &self,
        nonce: &[u8; XNONCE_LEN],
        aad: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, OpenError> {
        let (key, inner) = self.derive(nonce);
        decrypt(&key, inner, aad, ciphertext)
    }

    /// `nonce || ciphertext || tag` under a fresh random nonce.
    #[must_use]
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let nonce = quench_core::random::bytes::<XNONCE_LEN>();
        let mut out = nonce.to_vec();
        out.extend(self.encrypt(&nonce, aad, plaintext));
        out
    }

    /// Opens what [`seal`](Self::seal) produced.
    ///
    /// # Errors
    ///
    /// [`OpenError`] when it is too short or the tag does not verify.
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, OpenError> {
        let (nonce, ciphertext) = sealed.split_at_checked(XNONCE_LEN).ok_or(OpenError)?;
        let nonce: [u8; XNONCE_LEN] = nonce.try_into().map_err(|_| OpenError)?;
        self.decrypt(&nonce, aad, ciphertext)
    }
}

const fn quarter_round(state: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(16);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(12);
    state[a] = state[a].wrapping_add(state[b]);
    state[d] = (state[d] ^ state[a]).rotate_left(8);
    state[c] = state[c].wrapping_add(state[d]);
    state[b] = (state[b] ^ state[c]).rotate_left(7);
}

/// `HChaCha20`: the 20 `ChaCha` rounds over the key and a 16-byte nonce, keeping the first and last row (the
/// block counter is replaced by nonce bytes and nothing is added back), as the `XChaCha20` draft defines.
fn hchacha20(key: &[u8; KEY_LEN], nonce: &[u8]) -> [u8; KEY_LEN] {
    let word = |bytes: &[u8], index: usize| {
        let at = index * 4;
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let mut state = [0u32; 16];
    state[..4].copy_from_slice(&[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]);
    for index in 0..8 {
        state[4 + index] = word(key, index);
    }
    for index in 0..4 {
        state[12 + index] = word(nonce, index);
    }
    for _ in 0..10 {
        quarter_round(&mut state, 0, 4, 8, 12);
        quarter_round(&mut state, 1, 5, 9, 13);
        quarter_round(&mut state, 2, 6, 10, 14);
        quarter_round(&mut state, 3, 7, 11, 15);
        quarter_round(&mut state, 0, 5, 10, 15);
        quarter_round(&mut state, 1, 6, 11, 12);
        quarter_round(&mut state, 2, 7, 8, 13);
        quarter_round(&mut state, 3, 4, 9, 14);
    }
    let mut out = [0u8; KEY_LEN];
    for (index, value) in state[..4].iter().chain(&state[12..16]).enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(text: &str) -> Vec<u8> {
        quench_core::hex::decode(text.split_whitespace().collect::<String>()).unwrap()
    }

    #[test]
    fn hchacha20_matches_the_draft_vector() {
        let key: [u8; 32] =
            unhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .try_into()
                .unwrap();
        let nonce = unhex("000000090000004a0000000031415927");
        assert_eq!(
            quench_core::hex::encode(hchacha20(&key, &nonce)),
            "82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc"
        );
    }

    #[test]
    fn chacha20_poly1305_matches_rfc_8439() {
        // RFC 8439 section 2.8.2.
        let key: [u8; 32] =
            unhex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .try_into()
                .unwrap();
        let nonce: [u8; 12] = unhex("070000004041424344454647").try_into().unwrap();
        let aad = unhex("50515253c0c1c2c3c4c5c6c7");
        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let sealed = ChaCha20Poly1305::new(key).encrypt(nonce, &aad, plain);
        assert_eq!(
            quench_core::hex::encode(&sealed[sealed.len() - 16..]),
            "1ae10b594f09e26a7e902ecbd0600691"
        );
        assert!(quench_core::hex::encode(&sealed).starts_with("d31a8d34648e60db7b86afbc53ef7ec2"));
        assert_eq!(
            ChaCha20Poly1305::new(key)
                .decrypt(nonce, &aad, &sealed)
                .unwrap(),
            plain
        );
    }

    #[test]
    fn xchacha20_poly1305_matches_the_draft() {
        // draft-irtf-cfrg-xchacha appendix A.3.1.
        let key: [u8; 32] =
            unhex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .try_into()
                .unwrap();
        let nonce: [u8; 24] = unhex("404142434445464748494a4b4c4d4e4f5051525354555657")
            .try_into()
            .unwrap();
        let aad = unhex("50515253c0c1c2c3c4c5c6c7");
        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let sealed = XChaCha20Poly1305::new(key).encrypt(&nonce, &aad, plain);
        assert!(quench_core::hex::encode(&sealed).starts_with("bd6d179d3e83d43b9576579493c0e939"));
        assert_eq!(
            quench_core::hex::encode(&sealed[sealed.len() - 16..]),
            "c0875924c1c7987947deafd8780acf49"
        );
        assert_eq!(
            XChaCha20Poly1305::new(key)
                .decrypt(&nonce, &aad, &sealed)
                .unwrap(),
            plain
        );
    }

    #[test]
    fn seal_and_open_round_trip_and_bind_everything() {
        for (seal, open) in [
            (
                Box::new(|aad: &[u8], p: &[u8]| ChaCha20Poly1305::new([7; 32]).seal(aad, p))
                    as Box<dyn Fn(&[u8], &[u8]) -> Vec<u8>>,
                Box::new(|k: u8, aad: &[u8], s: &[u8]| ChaCha20Poly1305::new([k; 32]).open(aad, s))
                    as Box<dyn Fn(u8, &[u8], &[u8]) -> Result<Vec<u8>, OpenError>>,
            ),
            (
                Box::new(|aad: &[u8], p: &[u8]| XChaCha20Poly1305::new([7; 32]).seal(aad, p)),
                Box::new(|k: u8, aad: &[u8], s: &[u8]| {
                    XChaCha20Poly1305::new([k; 32]).open(aad, s)
                }),
            ),
        ] {
            let sealed = seal(b"ctx", b"secret");
            assert_eq!(open(7, b"ctx", &sealed).unwrap(), b"secret");
            assert_eq!(open(8, b"ctx", &sealed), Err(OpenError), "wrong key");
            assert_eq!(open(7, b"other", &sealed), Err(OpenError), "wrong context");
            let mut altered = sealed.clone();
            *altered.last_mut().unwrap() ^= 1;
            assert_eq!(open(7, b"ctx", &altered), Err(OpenError), "altered");
            assert_eq!(open(7, b"ctx", &sealed[..5]), Err(OpenError), "truncated");
            assert_ne!(seal(b"ctx", b"secret"), sealed, "a fresh nonce each time");
            assert_eq!(
                open(7, b"", &seal(b"", b"")).unwrap(),
                b"",
                "empty plaintext"
            );
        }
    }

    #[test]
    fn debug_hides_the_key_and_errors_display() {
        assert!(!format!("{:?}", ChaCha20Poly1305::new([9; 32])).contains('9'));
        assert!(!format!("{:?}", XChaCha20Poly1305::new([9; 32])).contains('9'));
        assert!(OpenError.to_string().contains("decryption failed"));
    }
}
