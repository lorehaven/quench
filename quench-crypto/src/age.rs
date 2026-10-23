//! File encryption in the [age](https://age-encryption.org/v1) format, to X25519 recipients.
//!
//! Compatible both ways with the `age` tool: what [`encrypt`] writes opens with `age --decrypt`, and what
//! `age` wrote to an `age1...` key opens with [`decrypt`]. Only X25519 recipients (`age1...` public keys,
//! `AGE-SECRET-KEY-1...` identities) are supported; passphrase (scrypt) and plugin recipients are not.
//!
//! A file is a header (one stanza per recipient, each wrapping the random 128-bit file key, and a MAC over
//! the whole) followed by the payload in 64 KiB ChaCha20-Poly1305 chunks. [`armor`] and [`dearmor`] convert
//! the binary form to and from the PEM-like text form.

use std::fmt;
use std::str::FromStr;

use quench_core::base64::{STANDARD, STANDARD_NO_PAD};
use quench_core::random;

use crate::aead::ChaCha20Poly1305;
use crate::hmac::{self, Algorithm};
use crate::{ct, hkdf, x25519};

const VERSION_LINE: &str = "age-encryption.org/v1";
const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const BEGIN: &str = "-----BEGIN AGE ENCRYPTED FILE-----";
const END: &str = "-----END AGE ENCRYPTED FILE-----";

/// Why a file could not be encrypted, decrypted or armoured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A key, recipient or armor that does not parse.
    Format(String),
    /// The header is malformed.
    Header(String),
    /// None of the identities fits any recipient stanza.
    NoMatchingIdentity,
    /// The header MAC is wrong: the header was changed, or the key was wrong.
    BadMac,
    /// The payload failed authentication, is truncated or has data after its end.
    Payload(String),
    /// There is no recipient to encrypt to.
    NoRecipients,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(m) => write!(f, "{m}"),
            Self::Header(m) => write!(f, "invalid age header: {m}"),
            Self::NoMatchingIdentity => {
                f.write_str("no identity matches any of the file's recipients")
            }
            Self::BadMac => f.write_str("the age header MAC is invalid"),
            Self::Payload(m) => write!(f, "invalid age payload: {m}"),
            Self::NoRecipients => f.write_str("at least one recipient is needed"),
        }
    }
}

impl std::error::Error for Error {}

// --- bech32 -------------------------------------------------------------------------------------

const CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn polymod(values: impl IntoIterator<Item = u8>) -> u32 {
    const GEN: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut chk = 1u32;
    for v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

fn hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out: Vec<u8> = hrp.bytes().map(|b| b >> 5).collect();
    out.push(0);
    out.extend(hrp.bytes().map(|b| b & 31));
    out
}

// Every pushed value is masked to `to` (at most 8) bits first, so the narrowing is exact.
#[allow(clippy::cast_possible_truncation)]
fn convert_bits(data: &[u8], from: u32, to: u32, pad: bool) -> Option<Vec<u8>> {
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::new();
    let max = (1u32 << to) - 1;
    for &value in data {
        if u32::from(value) >> from != 0 {
            return None;
        }
        acc = (acc << from) | u32::from(value);
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & max) as u8);
        }
    }
    if pad {
        if bits > 0 {
            out.push(((acc << (to - bits)) & max) as u8);
        }
    } else if bits >= from || ((acc << (to - bits)) & max) != 0 {
        return None;
    }
    Some(out)
}

fn bech32_encode(hrp: &str, data: &[u8]) -> String {
    let five = convert_bits(data, 8, 5, true).unwrap_or_default();
    let mut values = hrp_expand(hrp);
    values.extend(&five);
    values.extend([0u8; 6]);
    let checksum = polymod(values) ^ 1;
    let mut out = String::from(hrp);
    out.push('1');
    for v in five {
        out.push(char::from(CHARSET[usize::from(v)]));
    }
    for i in 0..6 {
        out.push(char::from(
            CHARSET[((checksum >> (5 * (5 - i))) & 31) as usize],
        ));
    }
    out
}

/// Decodes a bech32 string of a single case; `(hrp, data)`.
fn bech32_decode(text: &str) -> Result<(String, Vec<u8>), Error> {
    let bad = |why: &str| Error::Format(format!("not a valid bech32 string: {why}"));
    if text.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Err(bad("unexpected character"));
    }
    if text.chars().any(|c| c.is_ascii_lowercase()) && text.chars().any(|c| c.is_ascii_uppercase())
    {
        return Err(bad("mixed case"));
    }
    let lower = text.to_ascii_lowercase();
    let split = lower.rfind('1').ok_or_else(|| bad("no separator"))?;
    let (hrp, rest) = (&lower[..split], &lower[split + 1..]);
    if hrp.is_empty() || rest.len() < 6 {
        return Err(bad("too short"));
    }
    let mut five = Vec::with_capacity(rest.len());
    for c in rest.bytes() {
        let index = CHARSET
            .iter()
            .position(|&x| x == c)
            .ok_or_else(|| bad("unexpected character"))?;
        five.push(u8::try_from(index).unwrap_or(0));
    }
    let mut values = hrp_expand(hrp);
    values.extend(&five);
    if polymod(values) != 1 {
        return Err(bad("checksum mismatch"));
    }
    five.truncate(five.len() - 6);
    let data = convert_bits(&five, 5, 8, false).ok_or_else(|| bad("bad padding"))?;
    Ok((hrp.to_string(), data))
}

// --- keys ---------------------------------------------------------------------------------------

const PUBLIC_HRP: &str = "age";
const SECRET_HRP: &str = "age-secret-key-";

/// A public key files can be encrypted to (`age1...`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Recipient {
    key: [u8; 32],
}

impl Recipient {
    /// The raw 32-byte X25519 public key.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&bech32_encode(PUBLIC_HRP, &self.key))
    }
}

impl fmt::Debug for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Recipient({self})")
    }
}

impl FromStr for Recipient {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let (hrp, data) = bech32_decode(text)?;
        if hrp != PUBLIC_HRP {
            return Err(Error::Format(
                "not an age public key (it should start with `age1`)".into(),
            ));
        }
        let key: [u8; 32] = data
            .try_into()
            .map_err(|_| Error::Format("an age public key is 32 bytes".into()))?;
        Ok(Self { key })
    }
}

/// A private key files can be opened with (`AGE-SECRET-KEY-1...`).
#[derive(Clone)]
pub struct Identity {
    secret: [u8; 32],
}

impl Identity {
    /// A fresh random key.
    #[must_use]
    pub fn generate() -> Self {
        Self {
            secret: random::bytes::<32>(),
        }
    }

    /// The public key that goes with it.
    #[must_use]
    pub fn to_public(&self) -> Recipient {
        Recipient {
            key: x25519::public_key(&self.secret),
        }
    }

    /// The `AGE-SECRET-KEY-1...` text (keep it secret).
    #[must_use]
    pub fn to_secret_string(&self) -> String {
        bech32_encode(SECRET_HRP, &self.secret).to_ascii_uppercase()
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Identity(..)")
    }
}

impl FromStr for Identity {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let (hrp, data) = bech32_decode(text)?;
        if hrp != SECRET_HRP {
            return Err(Error::Format(
                "not an age secret key (it should start with `AGE-SECRET-KEY-1`)".into(),
            ));
        }
        let secret: [u8; 32] = data
            .try_into()
            .map_err(|_| Error::Format("an age secret key is 32 bytes".into()))?;
        Ok(Self { secret })
    }
}

// --- encryption ---------------------------------------------------------------------------------

fn wrap_key(
    shared: &[u8; 32],
    share: &[u8; 32],
    recipient: &[u8; 32],
) -> Result<ChaCha20Poly1305, Error> {
    let mut salt = Vec::with_capacity(64);
    salt.extend_from_slice(share);
    salt.extend_from_slice(recipient);
    let key = hkdf::sha256(shared, &salt, b"age-encryption.org/v1/X25519", 32)
        .map_err(|e| Error::Header(e.to_string()))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| Error::Header("bad key length".into()))?;
    Ok(ChaCha20Poly1305::new(key))
}

fn header_mac_key(file_key: &[u8; 16]) -> Vec<u8> {
    hkdf::sha256(file_key, &[], b"header", 32).unwrap_or_default()
}

fn payload_key(file_key: &[u8; 16], nonce: &[u8; 16]) -> ChaCha20Poly1305 {
    let key = hkdf::sha256(file_key, nonce, b"payload", 32).unwrap_or_default();
    ChaCha20Poly1305::new(key.try_into().unwrap_or([0; 32]))
}

fn chunk_nonce(counter: u64, last: bool) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[3..11].copy_from_slice(&counter.to_be_bytes());
    nonce[11] = u8::from(last);
    nonce
}

fn wrapped_base64(bytes: &[u8]) -> String {
    let text = STANDARD_NO_PAD.encode(bytes);
    let mut out = String::new();
    let mut rest = text.as_str();
    loop {
        let take = rest.len().min(64);
        out.push_str(&rest[..take]);
        out.push('\n');
        rest = &rest[take..];
        // A body whose last line is exactly 64 columns ends with an empty line.
        if rest.is_empty() && take < 64 {
            break;
        }
        if rest.is_empty() {
            out.push('\n');
            break;
        }
    }
    out
}

/// Encrypts `plaintext` so that any of `recipients` can open it. The result is the binary age format.
///
/// # Errors
///
/// [`Error::NoRecipients`] when `recipients` is empty.
pub fn encrypt(recipients: &[Recipient], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    if recipients.is_empty() {
        return Err(Error::NoRecipients);
    }
    let file_key = random::bytes::<16>();

    let mut header = format!("{VERSION_LINE}\n");
    for recipient in recipients {
        let ephemeral = random::bytes::<32>();
        let share = x25519::public_key(&ephemeral);
        let shared = x25519::x25519(&ephemeral, &recipient.key);
        let wrapped = wrap_key(&shared, &share, &recipient.key)?.encrypt([0; 12], &[], &file_key);
        header.push_str("-> X25519 ");
        header.push_str(&STANDARD_NO_PAD.encode(share));
        header.push('\n');
        header.push_str(&wrapped_base64(&wrapped));
    }
    header.push_str("---");
    let mac = hmac::sign(
        Algorithm::Sha256,
        &header_mac_key(&file_key),
        header.as_bytes(),
    );
    header.push(' ');
    header.push_str(&STANDARD_NO_PAD.encode(mac));
    header.push('\n');

    let nonce = random::bytes::<16>();
    let mut out = header.into_bytes();
    out.extend_from_slice(&nonce);
    let cipher = payload_key(&file_key, &nonce);
    let chunks: Vec<&[u8]> = if plaintext.is_empty() {
        vec![&[]]
    } else {
        plaintext.chunks(CHUNK).collect()
    };
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        out.extend(cipher.encrypt(chunk_nonce(index as u64, last), &[], chunk));
    }
    Ok(out)
}

/// One recipient stanza of a header.
struct Stanza {
    args: Vec<String>,
    body: Vec<u8>,
}

/// A parsed header: the stanzas, the MAC, the header bytes the MAC covers, and where the payload starts.
struct Header<'a> {
    stanzas: Vec<Stanza>,
    mac: Vec<u8>,
    covered: &'a [u8],
    payload_start: usize,
}

fn parse_header(data: &[u8]) -> Result<Header<'_>, Error> {
    let bad = |why: &str| Error::Header(why.to_string());
    let mut at = 0;
    let next_line = |at: &mut usize| -> Result<&str, Error> {
        let rest = data.get(*at..).ok_or_else(|| bad("truncated"))?;
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| bad("truncated"))?;
        let line = std::str::from_utf8(&rest[..end]).map_err(|_| bad("not text"))?;
        *at += end + 1;
        Ok(line)
    };
    if next_line(&mut at)? != VERSION_LINE {
        return Err(bad("unsupported version line"));
    }
    let mut stanzas = Vec::new();
    loop {
        let line_start = at;
        let line = next_line(&mut at)?;
        if let Some(mac_text) = line.strip_prefix("--- ") {
            let mac = STANDARD_NO_PAD
                .decode(mac_text)
                .map_err(|_| bad("unreadable MAC"))?;
            let covered = &data[..line_start + 3];
            return Ok(Header {
                stanzas,
                mac,
                covered,
                payload_start: at,
            });
        }
        let Some(arguments) = line.strip_prefix("-> ") else {
            return Err(bad("expected a stanza or the MAC line"));
        };
        let args: Vec<String> = arguments.split(' ').map(str::to_string).collect();
        if args.iter().any(String::is_empty) {
            return Err(bad("empty stanza argument"));
        }
        let mut body_text = String::new();
        loop {
            let line = next_line(&mut at)?;
            if line.len() > 64 {
                return Err(bad("stanza body line is longer than 64 columns"));
            }
            body_text.push_str(line);
            if line.len() < 64 {
                break;
            }
        }
        let body = STANDARD_NO_PAD
            .decode(&body_text)
            .map_err(|_| bad("unreadable stanza body"))?;
        stanzas.push(Stanza { args, body });
    }
}

/// Decrypts age data (binary or armored) with the first identity that fits.
///
/// # Errors
///
/// [`Error`] saying whether no identity fits, the header or payload was tampered with, or the data is not age.
pub fn decrypt(identities: &[Identity], data: &[u8]) -> Result<Vec<u8>, Error> {
    let owned;
    let data = if data.starts_with(BEGIN.as_bytes()) {
        owned = dearmor(&String::from_utf8_lossy(data))?;
        owned.as_slice()
    } else {
        data
    };
    let Header {
        stanzas,
        mac,
        covered,
        payload_start,
    } = parse_header(data)?;

    let mut file_key: Option<[u8; 16]> = None;
    'stanzas: for stanza in &stanzas {
        if stanza.args.len() != 2 || stanza.args[0] != "X25519" {
            continue;
        }
        let Ok(share) = STANDARD_NO_PAD.decode(&stanza.args[1]) else {
            return Err(Error::Header("unreadable X25519 share".into()));
        };
        let share: [u8; 32] = share
            .try_into()
            .map_err(|_| Error::Header("an X25519 share is 32 bytes".into()))?;
        if stanza.body.len() != 16 + TAG {
            return Err(Error::Header("an X25519 stanza wraps a 16-byte key".into()));
        }
        for identity in identities {
            let shared = x25519::x25519(&identity.secret, &share);
            if ct::eq(&shared, &[0u8; 32]) {
                // A low-order share gives no secret; age refuses it.
                continue;
            }
            let public = x25519::public_key(&identity.secret);
            let cipher = wrap_key(&shared, &share, &public)?;
            if let Ok(plain) = cipher.decrypt([0; 12], &[], &stanza.body)
                && let Ok(key) = <[u8; 16]>::try_from(plain.as_slice())
            {
                file_key = Some(key);
                break 'stanzas;
            }
        }
    }
    let file_key = file_key.ok_or(Error::NoMatchingIdentity)?;

    if !hmac::verify(Algorithm::Sha256, &header_mac_key(&file_key), covered, &mac) {
        return Err(Error::BadMac);
    }

    let payload = &data[payload_start..];
    let nonce: [u8; 16] = payload
        .get(..16)
        .ok_or_else(|| Error::Payload("missing nonce".into()))?
        .try_into()
        .map_err(|_| Error::Payload("missing nonce".into()))?;
    let cipher = payload_key(&file_key, &nonce);
    let mut rest = &payload[16..];
    if rest.is_empty() {
        return Err(Error::Payload("no data".into()));
    }
    let mut out = Vec::new();
    let mut counter = 0u64;
    while !rest.is_empty() {
        let take = rest.len().min(CHUNK + TAG);
        let (chunk, after) = rest.split_at(take);
        let last = after.is_empty();
        let plain = cipher
            .decrypt(chunk_nonce(counter, last), &[], chunk)
            .map_err(|_| Error::Payload("authentication failed".into()))?;
        if last && plain.is_empty() && counter > 0 {
            return Err(Error::Payload("an empty final chunk after data".into()));
        }
        out.extend(plain);
        counter += 1;
        rest = after;
    }
    Ok(out)
}

/// The text form of binary age data.
#[must_use]
pub fn armor(data: &[u8]) -> String {
    let text = STANDARD.encode(data);
    let mut out = String::from(BEGIN);
    out.push('\n');
    for line in text.as_bytes().chunks(64) {
        out.push_str(&String::from_utf8_lossy(line));
        out.push('\n');
    }
    out.push_str(END);
    out.push('\n');
    out
}

/// The binary age data inside armor.
///
/// # Errors
///
/// [`Error::Format`] when the armor lines or the base64 inside are malformed.
pub fn dearmor(text: &str) -> Result<Vec<u8>, Error> {
    let bad = |why: &str| Error::Format(format!("invalid age armor: {why}"));
    let mut lines = text.lines().map(str::trim_end);
    if lines.next() != Some(BEGIN) {
        return Err(bad("missing the BEGIN line"));
    }
    let mut body = String::new();
    loop {
        match lines.next() {
            Some(line) if line == END => break,
            Some(line) if line.len() <= 64 => body.push_str(line),
            Some(_) => return Err(bad("a line is longer than 64 columns")),
            None => return Err(bad("missing the END line")),
        }
    }
    if lines.any(|l| !l.is_empty()) {
        return Err(bad("data after the END line"));
    }
    STANDARD.decode(&body).map_err(|_| bad("unreadable base64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str =
        "AGE-SECRET-KEY-1GQ9778VQXMMJVE8SK7J6VT8UJ4HDQAJUVSFCWCM02D8GEWQ72PVQ2Y5J33";
    const PUBLIC: &str = "age1t7rxyev2z3rw82stdlrrepyc39nvn86l5078zqkf5uasdy86jp6svpy7pa";

    #[test]
    fn keys_encode_and_decode() {
        let identity: Identity = SECRET.parse().expect("a valid secret key");
        assert_eq!(identity.to_public().to_string(), PUBLIC);
        assert_eq!(identity.to_secret_string(), SECRET);
        let recipient: Recipient = PUBLIC.parse().expect("a valid public key");
        assert_eq!(recipient, identity.to_public());
        assert_eq!(recipient.as_bytes(), &x25519::public_key(&identity.secret));
        assert!(
            format!("{identity:?}") == "Identity(..)" && format!("{recipient:?}").contains(PUBLIC)
        );

        let fresh = Identity::generate();
        let again: Identity = fresh.to_secret_string().parse().unwrap();
        assert_eq!(again.to_public(), fresh.to_public());
        assert_ne!(Identity::generate().to_public(), fresh.to_public());
        assert_eq!(PUBLIC.to_uppercase().parse::<Recipient>(), Ok(recipient));
    }

    #[test]
    fn bad_keys_are_rejected() {
        for bad in [
            "",
            "age1",
            PUBLIC.replace('q', "z").as_str(),
            "age1ql3z7hjy",
            "AGE-SECRET-KEY-1GQ9778VQXMMJVE8SK7J6VT8UJ4HDQAJUVSFCWCM02D8GEWQ72PVQ2Y5J3",
            "Age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p",
            "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8b",
            "noseparator",
            "1qqqqqq",
            "age1\u{e9}qqqqqqqq",
        ] {
            assert!(bad.parse::<Recipient>().is_err(), "{bad:?}");
        }
        assert!(
            SECRET.parse::<Recipient>().is_err(),
            "a secret key is not a recipient"
        );
        assert!(
            PUBLIC.parse::<Identity>().is_err(),
            "a public key is not an identity"
        );
        assert!(SECRET.to_lowercase().parse::<Identity>().is_ok());
        assert!(
            "AGE-SECRET-KEY-1GQ9778VQXMMJVE8SK7J6VT8UJ4HDQAJUVSFCWCM02D8GEWQ72PVQ2Y5J33X"
                .parse::<Identity>()
                .is_err()
        );
        assert!(
            bech32_decode("A12UEL5L").is_ok() && bech32_decode("a12uel5l").is_ok(),
            "the BIP-173 empty-data vector"
        );
        assert!(bech32_decode("A12uEL5L").is_err(), "mixed case");
    }

    #[test]
    fn encrypt_then_decrypt() {
        let identity = Identity::generate();
        for size in [
            0usize,
            1,
            100,
            CHUNK - 1,
            CHUNK,
            CHUNK + 1,
            3 * CHUNK,
            3 * CHUNK + 17,
        ] {
            let plain: Vec<u8> = (0..size).map(|i| u8::try_from(i % 251).unwrap()).collect();
            let sealed = encrypt(&[identity.to_public()], &plain).unwrap();
            assert!(sealed.starts_with(b"age-encryption.org/v1\n-> X25519 "));
            assert_eq!(
                decrypt(std::slice::from_ref(&identity), &sealed).unwrap(),
                plain,
                "size {size}"
            );
        }
        // Two encryptions of the same data differ (random file key, ephemeral key and nonce).
        let a = encrypt(&[identity.to_public()], b"same").unwrap();
        let b = encrypt(&[identity.to_public()], b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn several_recipients_each_open_it() {
        let (a, b, c) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let sealed = encrypt(&[a.to_public(), b.to_public()], b"for two").unwrap();
        assert_eq!(
            decrypt(std::slice::from_ref(&a), &sealed).unwrap(),
            b"for two"
        );
        assert_eq!(
            decrypt(std::slice::from_ref(&b), &sealed).unwrap(),
            b"for two"
        );
        assert_eq!(
            decrypt(&[c.clone(), b], &sealed).unwrap(),
            b"for two",
            "any identity in the list"
        );
        assert_eq!(decrypt(&[c], &sealed), Err(Error::NoMatchingIdentity));
        assert_eq!(decrypt(&[], &sealed), Err(Error::NoMatchingIdentity));
        assert_eq!(encrypt(&[], b"x"), Err(Error::NoRecipients));
    }

    #[test]
    fn tampering_is_detected() {
        let identity = Identity::generate();
        let sealed = encrypt(&[identity.to_public()], b"secret value").unwrap();
        let ids = [identity];

        // The payload: any flipped bit, a truncation, an extension.
        for at in [sealed.len() - 1, sealed.len() - 20, sealed.len() - 30] {
            let mut copy = sealed.clone();
            copy[at] ^= 1;
            assert!(
                matches!(decrypt(&ids, &copy), Err(Error::Payload(_))),
                "byte {at}"
            );
        }
        assert!(matches!(
            decrypt(&ids, &sealed[..sealed.len() - 1]),
            Err(Error::Payload(_))
        ));
        let mut longer = sealed.clone();
        longer.push(0);
        assert!(matches!(decrypt(&ids, &longer), Err(Error::Payload(_))));
        // Only the header and nonce left.
        let header_end = sealed.windows(5).position(|w| w == b"\n--- ").unwrap();
        let payload_start = sealed[header_end + 1..]
            .iter()
            .position(|&b| b == b'\n')
            .unwrap()
            + header_end
            + 2;
        assert!(matches!(
            decrypt(&ids, &sealed[..payload_start]),
            Err(Error::Payload(_))
        ));
        assert!(matches!(
            decrypt(&ids, &sealed[..payload_start + 16]),
            Err(Error::Payload(_))
        ));
        assert!(matches!(
            decrypt(&ids, &sealed[..payload_start + 5]),
            Err(Error::Payload(_))
        ));

        // The header: the MAC covers all of it.
        let mut copy = sealed.clone();
        let mac_at = header_end + 5;
        copy[mac_at] = if copy[mac_at] == b'A' { b'B' } else { b'A' };
        assert_eq!(decrypt(&ids, &copy), Err(Error::BadMac));
        let mut renamed = String::from_utf8_lossy(&sealed[..header_end])
            .replace("-> X25519", "-> grease")
            .into_bytes();
        renamed.extend_from_slice(&sealed[header_end..]);
        assert_eq!(decrypt(&ids, &renamed), Err(Error::NoMatchingIdentity));
    }

    #[test]
    fn malformed_headers_are_refused() {
        let ids = [Identity::generate()];
        for bad in [
            &b""[..],
            b"not age\n",
            b"age-encryption.org/v1\n",
            b"age-encryption.org/v1\nbogus\n",
            b"age-encryption.org/v1\n-> X25519 !!!\nAAAA\n--- AAAA\n",
            b"age-encryption.org/v1\n-> X25519 AAAA\n!!!\n--- AAAA\n",
            b"age-encryption.org/v1\n-> X25519 \nAAAA\n--- AAAA\n",
            b"age-encryption.org/v1\n-> X25519 AAAA\nAAAA\n--- !!\n",
            &[b'a', b'g', b'e', 0xff, b'\n'],
        ] {
            assert!(
                matches!(decrypt(&ids, bad), Err(Error::Header(_))),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
        // A stanza of another type is skipped; a malformed X25519 one is an error.
        let wrong_size = b"age-encryption.org/v1\n-> X25519 ".to_vec();
        let mut text = wrong_size;
        text.extend_from_slice(STANDARD_NO_PAD.encode([1u8; 32]).as_bytes());
        text.extend_from_slice(b"\nAAAA\n--- AAAA\n");
        assert!(matches!(decrypt(&ids, &text), Err(Error::Header(_))));
        let long_body = format!(
            "age-encryption.org/v1\n-> X25519 {}\n{}\n--- AAAA\n",
            STANDARD_NO_PAD.encode([1u8; 32]),
            "A".repeat(70)
        );
        assert!(matches!(
            decrypt(&ids, long_body.as_bytes()),
            Err(Error::Header(_))
        ));
    }

    #[test]
    fn armor_round_trips() {
        let identity = Identity::generate();
        let sealed = encrypt(&[identity.to_public()], &vec![7u8; 5000]).unwrap();
        let text = armor(&sealed);
        assert!(text.starts_with("-----BEGIN AGE ENCRYPTED FILE-----\n"));
        assert!(text.ends_with("-----END AGE ENCRYPTED FILE-----\n"));
        assert!(text.lines().all(|l| l.len() <= 64));
        assert_eq!(dearmor(&text).unwrap(), sealed);
        assert_eq!(
            decrypt(&[identity], text.as_bytes()).unwrap(),
            vec![7u8; 5000]
        );
        for bad in [
            "",
            "no begin\n",
            "-----BEGIN AGE ENCRYPTED FILE-----\nAAAA\n",
            &format!("{BEGIN}\n{}\n{END}\n", "A".repeat(80)),
            &format!("{BEGIN}\n!!!!\n{END}\n"),
            &format!("{BEGIN}\nAAAA\n{END}\ntrailing\n"),
        ] {
            assert!(matches!(dearmor(bad), Err(Error::Format(_))), "{bad:?}");
        }
    }

    #[test]
    fn wrapped_stanza_bodies_follow_the_64_column_rule() {
        assert_eq!(wrapped_base64(&[0u8; 32]).lines().count(), 1);
        // 48 bytes encode to exactly 64 columns: the body then ends with an empty line.
        let exact = wrapped_base64(&[1u8; 48]);
        assert!(
            exact.ends_with("\n\n") && exact.lines().next().map(str::len) == Some(64),
            "{exact:?}"
        );
        assert_eq!(wrapped_base64(&[]), "\n");
    }

    #[test]
    fn errors_describe_themselves() {
        for error in [
            Error::Format("f".into()),
            Error::Header("h".into()),
            Error::NoMatchingIdentity,
            Error::BadMac,
            Error::Payload("p".into()),
            Error::NoRecipients,
        ] {
            assert!(!error.to_string().is_empty());
        }
    }
}
