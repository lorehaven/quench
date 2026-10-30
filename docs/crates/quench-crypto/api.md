---
title: API reference
parent: quench-crypto
nav_order: 2
---

Every public item, in source (module) order of `lib.rs`. Modules behind a feature say so. All types are `Send + Sync` (they hold plain bytes). Signatures are copied from the source. Unless stated, functions do not panic.

## `aead` (feature `aead`)

ChaCha20-Poly1305 (RFC 8439) and XChaCha20-Poly1305. Output formats: `encrypt` returns `ciphertext || tag`; `seal` returns `nonce || ciphertext || tag`.

```rust
pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;   // ChaCha20Poly1305
pub const XNONCE_LEN: usize = 24;  // XChaCha20Poly1305
pub const TAG_LEN: usize = 16;
```

### `OpenError`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenError;   // Display, std::error::Error
```

Any failure to open: wrong key, wrong associated data, truncated or altered ciphertext, a `sealed` value shorter than the nonce. It deliberately does not say which. Display text: `decryption failed: wrong key or associated data, or the ciphertext was altered`.

### `ChaCha20Poly1305`

```rust
#[derive(Clone)] pub struct ChaCha20Poly1305 { /* key */ }
impl ChaCha20Poly1305 {
    #[must_use] pub const fn new(key: [u8; KEY_LEN]) -> Self;
    #[must_use] pub fn encrypt(&self, nonce: [u8; NONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8>;
    pub fn decrypt(&self, nonce: [u8; NONCE_LEN], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, OpenError>;
    #[must_use] pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Vec<u8>;
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, OpenError>;
}
```

* `new`: stores the key. `Debug` prints `ChaCha20Poly1305 { .. }` (never the key). The key is not zeroized on drop.
* `encrypt`: a nonce must never be used twice with one key; the caller owns that rule. Returns plaintext length + 16 bytes.
* `decrypt`: `OpenError` when the tag does not verify (also when `ciphertext` is shorter than 16 bytes).
* `seal`: draws a fresh 12-byte nonce from `quench_core::random` and prepends it. `aad` must be passed again to `open`.
* `open`: reads the first 12 bytes as the nonce; `OpenError` when shorter than 12 bytes or the tag fails.

### `XChaCha20Poly1305`

```rust
#[derive(Clone)] pub struct XChaCha20Poly1305 { /* key */ }
impl XChaCha20Poly1305 {
    #[must_use] pub const fn new(key: [u8; KEY_LEN]) -> Self;
    #[must_use] pub fn encrypt(&self, nonce: &[u8; XNONCE_LEN], aad: &[u8], plaintext: &[u8]) -> Vec<u8>;
    pub fn decrypt(&self, nonce: &[u8; XNONCE_LEN], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, OpenError>;
    #[must_use] pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Vec<u8>;
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, OpenError>;
}
```

Same contract with a 24-byte nonce (random nonces are safe). Note the asymmetry: the nonce is passed by value (`[u8; 12]`) for `ChaCha20Poly1305` and by reference (`&[u8; 24]`) for `XChaCha20Poly1305`. `Debug` prints `XChaCha20Poly1305 { .. }`.

## `age` (feature `age`)

age v1 file encryption to X25519 recipients (binary and ASCII-armored). See [internals](internals.md#age) for what is and is not supported.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Format(String),        // a key, recipient or armor that does not parse
    Header(String),        // the header is malformed
    NoMatchingIdentity,    // no identity fits any recipient stanza
    BadMac,                // header MAC wrong: header changed, or wrong key
    Payload(String),       // payload failed authentication, truncated, or trailing data
    NoRecipients,          // encrypt called with an empty recipient list
}                          // Display, std::error::Error
```

Display: `Format(m)` prints `m`; `Header(m)` -> `invalid age header: m`; `NoMatchingIdentity` -> `no identity matches any of the file's recipients`; `BadMac` -> `the age header MAC is invalid`; `Payload(m)` -> `invalid age payload: m`; `NoRecipients` -> `at least one recipient is needed`.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)] pub struct Recipient { /* key: [u8; 32] */ }
impl Recipient { #[must_use] pub const fn as_bytes(&self) -> &[u8; 32]; }
impl fmt::Display for Recipient;               // the `age1...` bech32 text
impl fmt::Debug for Recipient;                 // `Recipient(age1...)`
impl FromStr for Recipient { type Err = Error; }
```

`from_str` fails with `Error::Format` when the bech32 is invalid, the human-readable part is not `age` ("not an age public key (it should start with `age1`)") or the payload is not 32 bytes ("an age public key is 32 bytes").

```rust
#[derive(Clone)] pub struct Identity { /* secret: [u8; 32] */ }
impl Identity {
    #[must_use] pub fn generate() -> Self;
    #[must_use] pub fn to_public(&self) -> Recipient;
    #[must_use] pub fn to_secret_string(&self) -> String;   // AGE-SECRET-KEY-1...
}
impl fmt::Debug for Identity;                  // never prints the key
impl FromStr for Identity { type Err = Error; }
```

`generate` uses 32 random bytes. `from_str` expects the human-readable part `age-secret-key-` and 32 bytes (`Error::Format` otherwise); bech32 text is accepted in a single case.

```rust
pub fn encrypt(recipients: &[Recipient], plaintext: &[u8]) -> Result<Vec<u8>, Error>;
pub fn decrypt(identities: &[Identity], data: &[u8]) -> Result<Vec<u8>, Error>;
#[must_use] pub fn armor(data: &[u8]) -> String;
pub fn dearmor(text: &str) -> Result<Vec<u8>, Error>;
```

* `encrypt`: binary age. A random 16-byte file key; one `-> X25519` stanza per recipient with a fresh ephemeral key; header MAC; 16-byte payload nonce; payload in 64 KiB chunks. `Error::NoRecipients` for an empty slice. An empty plaintext produces one empty (final) chunk.
* `decrypt`: accepts armored data too (anything that starts with `-----BEGIN AGE ENCRYPTED FILE-----` is de-armored first; invalid UTF-8 is replaced lossily). Tries the stanzas against the identities; first fit wins. Verifies the header MAC and every chunk. Errors: `Header`, `NoMatchingIdentity`, `BadMac`, `Payload`, `Format` (armor).
* `armor`: `-----BEGIN AGE ENCRYPTED FILE-----`, base64 standard with padding wrapped at 64 columns, `-----END AGE ENCRYPTED FILE-----`.
* `dearmor`: `Error::Format` when the armor lines or the base64 inside are malformed.

## `argon2` (feature `argon2`)

Argon2id, version 0x13 only (RFC 9106). The BLAKE2b module it uses (`blake2b`) is private.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    pub memory_kib: u32,   // memory in KiB
    pub iterations: u32,   // passes over memory
    pub lanes: u32,        // degree of parallelism
    pub output_len: usize, // hash length in bytes
}
impl Default for Params;   // memory_kib 19*1024 (19456), iterations 2, lanes 1, output_len 32
```

Valid ranges (checked by every hashing function, error `ParamsError`):

| Field / input | Rule | Error text (after `invalid Argon2 parameters: `) |
|---|---|---|
| `lanes` | 1 ..= 16 777 215 | `lanes must be 1 to 16777215` |
| `iterations` | >= 1 | `iterations must be at least 1` |
| `memory_kib` | >= 8 * `lanes` | `memory must be at least 8 KiB per lane` |
| `output_len` | >= 4 (and fits `u32`) | `the output must be at least 4 bytes` |
| salt | >= 8 bytes | `the salt must be at least 8 bytes` |

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct ParamsError(&'static str); // Display: "invalid Argon2 parameters: {0}"
#[derive(Debug, Clone, PartialEq, Eq)] pub struct PhcError(&'static str);    // Display: "invalid Argon2 hash string: {0}"

pub fn hash_raw(password: &[u8], salt: &[u8], params: &Params) -> Result<Vec<u8>, ParamsError>;
pub fn hash_raw_keyed(password: &[u8], salt: &[u8], secret: &[u8], associated_data: &[u8], params: &Params) -> Result<Vec<u8>, ParamsError>;
pub fn hash_password(password: &str, params: &Params) -> Result<String, ParamsError>;
pub fn hash_password_with_salt(password: &str, salt: &[u8], params: &Params) -> Result<String, ParamsError>;
pub fn verify_password(password: &str, phc: &str) -> Result<bool, PhcError>;
```

* `hash_raw`: the raw tag of `output_len` bytes. Equals `hash_raw_keyed` with empty secret and associated data.
* `hash_raw_keyed`: with the optional secret ("pepper") and associated data of RFC 9106. Memory actually used: `4 * lanes * (memory_kib / (4 * lanes))` blocks of 1 KiB.
* `hash_password`: 16 random salt bytes (from `quench_core::random`); returns `$argon2id$v=19$m=<memory_kib>,t=<iterations>,p=<lanes>$<salt>$<hash>` with unpadded standard base64.
* `hash_password_with_salt`: the same with a caller-supplied salt (tests, migrations).
* `verify_password`: parses the PHC string, recomputes with the stored parameters (`output_len` = the stored hash length) and compares in constant time (`ct::eq`). `Ok(false)` is a wrong password. `Err(PhcError)`: see [errors](errors.md#phcerror).

## `ct`

```rust
#[must_use] pub fn eq(a: &[u8], b: &[u8]) -> bool;
```

Different lengths are unequal (returned early: the length is not treated as secret). Equal lengths: all bytes are examined, differences OR-ed, result passed through `std::hint::black_box`. See [internals](internals.md#constant-time-claims) for the limits of this claim.

## `ed25519` (feature `ed25519`)

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct KeyError;   // Display: "not a valid Ed25519 private key"

pub struct SigningKey { /* seed: [u8; 32], pair: ring Ed25519KeyPair */ }
impl SigningKey {
    #[must_use] pub fn from_seed(seed: &[u8; 32]) -> Self;
    #[must_use] pub fn generate() -> Self;
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self, KeyError>;
    #[must_use] pub fn to_pkcs8_der(&self) -> Vec<u8>;
    #[must_use] pub const fn seed(&self) -> &[u8; 32];
    #[must_use] pub fn public_key(&self) -> [u8; 32];
    #[must_use] pub fn sign(&self, message: &[u8]) -> [u8; 64];
}
#[must_use] pub fn verify(public_key: &[u8], message: &[u8], signature: &[u8]) -> bool;
```

* `from_seed`: every 32-byte seed is a valid key. `generate`: seed from OS randomness.
* `from_pkcs8_der`: unencrypted PKCS#8, v1 (48 bytes, seed only) or v2 (83 bytes, public key attached). The seed is read from bytes 16..48 of the input. `KeyError` when `ring` rejects the document.
* `to_pkcs8_der`: always v2, 83 bytes, byte-identical to `ed25519-dalek`'s layout (see [internals](internals.md#ed25519-pkcs8-layout)).
* `seed`: the raw seed (secret). `Debug` prints only the hex public key (`SigningKey { public_key: .., .. }`).
* `sign`: deterministic Ed25519 signature (RFC 8032).
* `verify`: `false` for any malformed key, message or signature length (including non-canonical signatures, as implemented by `ring`).

## `hash`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Algorithm { Sha256, Sha384, Sha512 }
impl Algorithm { #[must_use] pub const fn output_len(self) -> usize; }  // 32, 48, 64

#[must_use] pub fn digest(algorithm: Algorithm, data: impl AsRef<[u8]>) -> Vec<u8>;
#[must_use] pub fn sha256(data: impl AsRef<[u8]>) -> [u8; 32];
#[must_use] pub fn sha384(data: impl AsRef<[u8]>) -> [u8; 48];
#[must_use] pub fn sha512(data: impl AsRef<[u8]>) -> [u8; 64];
#[must_use] pub fn sha1_legacy(data: impl AsRef<[u8]>) -> [u8; 20];

#[derive(Clone)] pub struct Hasher(/* ring Context */);
impl Hasher {
    #[must_use] pub fn new(algorithm: Algorithm) -> Self;
    pub fn update(&mut self, data: impl AsRef<[u8]>);
    #[must_use] pub fn finish(self) -> Vec<u8>;
}
```

`sha1_legacy` is for protocols that mandate SHA-1 (the WebSocket handshake, RFC 6455). `Hasher` is `Clone` (fork a running hash) and `Debug` (opaque). Note this `Algorithm` has no SHA-1; `hmac::Algorithm` does.

## `hkdf`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct TooLong;  // Display: "HKDF output length is more than 255 * 32 bytes"
pub fn sha256(ikm: &[u8], salt: &[u8], info: &[u8], length: usize) -> Result<Vec<u8>, TooLong>;
```

HKDF (RFC 5869) with HMAC-SHA-256. `salt` may be empty. `TooLong` when `length` > 8160 (255 * 32).

## `hmac`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Algorithm { Sha1, Sha256, Sha384, Sha512 }
#[must_use] pub fn sign(algorithm: Algorithm, key: &[u8], data: impl AsRef<[u8]>) -> Vec<u8>;
#[must_use] pub fn verify(algorithm: Algorithm, key: &[u8], data: impl AsRef<[u8]>, tag: &[u8]) -> bool;
```

`Sha1` is `HMAC_SHA1_FOR_LEGACY_USE_ONLY` (for TOTP and fixed protocols). `verify` is `ring::hmac::verify`, which compares in constant time. Keys of any length are accepted.

## `jwt` (feature `jwt`)

JSON Web Tokens over compact JWS. Supported algorithms: `EdDSA`, `HS256`, `HS384`, `HS512`, `RS256` (verify only).

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidToken, InvalidSignature, InvalidAlgorithm, InvalidKeyFormat,
    ExpiredSignature, ImmatureSignature,
    MissingRequiredClaim(String), InvalidAudience, InvalidIssuer,
    Base64(String), Json(String),
}
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Error { /* kind */ }
impl Error { #[must_use] pub const fn kind(&self) -> &ErrorKind; }
impl From<ErrorKind> for Error;   // Display, std::error::Error
pub type Result<T> = std::result::Result<T, Error>;
```

See [errors](errors.md#jwt-errors) for when each kind occurs and the Display texts.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm { EdDSA, HS256, HS384, HS512, RS256 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header { pub alg: Algorithm, pub typ: Option<String>, pub kid: Option<String> }
impl Header { #[must_use] pub fn new(alg: Algorithm) -> Self; }   // typ = Some("JWT"), kid = None
```

When encoding, the header JSON key order is `typ`, `alg`, `kid` (`typ` and `kid` only when `Some`). When decoding, `alg` is required (missing or not a string: `InvalidToken`; unknown name, including `none`: `InvalidAlgorithm`), `typ`/`kid` are read when they are strings.

```rust
pub enum EncodingKey { Ed25519(SigningKey), Hmac(Vec<u8>) }     // Debug never prints key material
impl EncodingKey {
    pub fn from_ed_der(pkcs8: &[u8]) -> Result<Self>;            // InvalidKeyFormat on a bad key
    #[must_use] pub const fn from_ed_key(key: SigningKey) -> Self;
    #[must_use] pub fn from_secret(secret: &[u8]) -> Self;       // HMAC secret
}

#[derive(Clone, PartialEq, Eq)]
pub enum DecodingKey { Ed25519(Vec<u8>), Hmac(Vec<u8>), Rsa(Vec<u8>) }   // Debug never prints key material
impl DecodingKey {
    #[must_use] pub fn from_ed_der(public_key: &[u8]) -> Self;   // despite the name: the raw 32-byte public key
    pub fn from_ed_components(x: &str) -> Result<Self>;          // JWK `x`, base64url (Base64 error otherwise)
    #[must_use] pub fn from_secret(secret: &[u8]) -> Self;
    pub fn from_rsa_components(n: &str, e: &str) -> Result<Self>; // JWK `n`,`e`, base64url -> DER RSAPublicKey
}
```

`from_ed_der` takes the **raw 32-byte public key**, not DER (the name follows `jsonwebtoken`). `DecodingKey::Rsa` holds a DER `RSAPublicKey`.

```rust
#[derive(Debug, Clone)]
pub struct Validation {
    pub algorithms: Vec<Algorithm>,              // accepted header algorithms
    pub leeway: u64,                             // seconds tolerated for exp and nbf
    pub validate_exp: bool,
    pub validate_nbf: bool,
    pub validate_aud: bool,                      // when on, `aud` must contain one of `aud`
    pub aud: Option<BTreeSet<String>>,
    pub iss: Option<BTreeSet<String>>,
    pub required_spec_claims: BTreeSet<String>,  // claims that must be present
}
impl Validation { #[must_use] pub fn new(algorithm: Algorithm) -> Self; }

#[derive(Debug, Clone)] pub struct TokenData<T> { pub header: Header, pub claims: T }
```

`Validation::new(algorithm)`: `algorithms = [algorithm]`, `leeway = 60`, `validate_exp = true`, `validate_nbf = false`, `validate_aud = true`, `aud = None`, `iss = None`, `required_spec_claims = {"exp"}`.

```rust
pub fn encode<T: Serialize + ?Sized>(header: &Header, claims: &T, key: &EncodingKey) -> Result<String>;
pub fn decode_header(token: &str) -> Result<Header>;
pub fn verify(token: &str, key: &DecodingKey, algorithms: &[Algorithm]) -> Result<(Header, Vec<u8>)>;
pub fn decode<T: DeserializeOwned>(token: &str, key: &DecodingKey, validation: &Validation) -> Result<TokenData<T>>;
```

* `encode`: `base64url(header JSON) "." base64url(claims JSON) "." base64url(signature)`, no padding. `header.alg` and `key` must match: `EdDSA` with `Ed25519`, `HS256/384/512` with `Hmac`; anything else (including `RS256`, which cannot be signed) is `InvalidAlgorithm`.
* `decode_header`: parses the header only; nothing is verified. Use it to pick a key by `kid`.
* `verify`: checks structure, that the header algorithm is in `algorithms`, then the signature, and returns the header and the decoded payload bytes. Looks at no claims. Key family must match the header algorithm, otherwise `InvalidAlgorithm`. HMAC tags are compared with `ct::eq`.
* `decode`: `verify` with `validation.algorithms`, then parses the payload as JSON, runs the claim checks (see [internals](internals.md#jwt)) using `quench_core::time::Time::now()`, then deserializes into `T` (`Json` error if the shape is wrong).

## `pbkdf2`

```rust
pub fn derive(algorithm: hmac::Algorithm, iterations: u32, salt: &[u8], password: &[u8], out: &mut [u8]);
#[must_use] pub fn verify(algorithm: hmac::Algorithm, iterations: u32, salt: &[u8], password: &[u8], expected: &[u8]) -> bool;
```

The `algorithm` type is `crate::hmac::Algorithm` (SHA-1/256/384/512). `iterations == 0` is treated as 1 by `derive`. `out.len()` is the derived key length. `verify` is ring's constant-time check.

## `rsa` (feature `rsa`)

```rust
#[must_use] pub fn verify_pkcs1_sha256(public_key_der: &[u8], message: &[u8], signature: &[u8]) -> bool;
#[must_use] pub fn public_key_from_components(n: &[u8], e: &[u8]) -> Vec<u8>;
```

RSASSA-PKCS1-v1_5 with SHA-256 (`RSA_PKCS1_2048_8192_SHA256`: keys of 2048 to 8192 bits). Verification only; no RSA private key handling. `public_key_from_components` builds the DER `RSAPublicKey` (`SEQUENCE { INTEGER n, INTEGER e }`) from big-endian `n` and `e`, stripping leading zeros and adding one zero byte when the top bit is set.

## `totp` (feature `totp`)

```rust
pub const MIN_SECRET_BYTES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Algorithm { Sha1, Sha256, Sha512 }
impl Algorithm { #[must_use] pub const fn name(self) -> &'static str; }  // "SHA1", "SHA256", "SHA512"

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error { SecretTooShort(usize), BadBase32(quench_core::base32::DecodeError), BadDigits(u32) }

#[must_use] pub fn generate_secret() -> Vec<u8>;
#[must_use] pub fn hotp(algorithm: Algorithm, key: &[u8], counter: u64, digits: u32) -> String;

#[derive(Clone, PartialEq, Eq)] pub struct Totp { /* secret, algorithm, digits, step, skew */ }
impl Totp {
    pub fn new(secret: Vec<u8>) -> Result<Self, Error>;
    pub fn from_base32(secret: &str) -> Result<Self, Error>;
    #[must_use] pub const fn with_algorithm(self, algorithm: Algorithm) -> Self;
    pub fn with_digits(self, digits: u32) -> Result<Self, Error>;
    #[must_use] pub const fn with_step(self, seconds: u64) -> Self;
    #[must_use] pub const fn with_skew(self, steps: u64) -> Self;
    #[must_use] pub fn secret_base32(&self) -> String;
    #[must_use] pub fn code_at(&self, unix_seconds: u64) -> String;
    #[must_use] pub fn code_now(&self) -> String;
    #[must_use] pub fn check_at(&self, code: &str, unix_seconds: u64) -> bool;
    #[must_use] pub fn check_now(&self, code: &str) -> bool;
    #[must_use] pub fn url(&self, issuer: &str, account: &str) -> String;
}
```

* Defaults of `new`: SHA-1, 6 digits, 30-second step, skew 1.
* `hotp`: RFC 4226 dynamic truncation; `digits` is clamped to at most 9 (`digits.min(9)`); the result is zero-padded.
* `with_step`: a step of 0 is stored as 1. `with_digits`: `BadDigits` outside 6..=9.
* `check_at`: trims surrounding whitespace of `code`; computes **every** window `counter-skew ..= counter+skew` and compares each in constant time (no early exit). `check_now` uses `SystemTime::now()` (0 if the clock is before the epoch).
* `url`: `otpauth://totp/<issuer>:<account>?secret=<base32>&issuer=<issuer>&algorithm=<NAME>&digits=<n>&period=<step>`; issuer and account are percent-encoded except RFC 3986 unreserved characters (`A-Z a-z 0-9 - . _ ~`), hex in upper case.
* `Debug` hides the secret. The secret is not zeroized on drop. Replay (reusing an accepted code in the same window) is not tracked.

## `x25519` (feature `x25519`)

```rust
pub const BASEPOINT: [u8; 32];     // u = 9
#[must_use] pub fn x25519(scalar: &[u8; 32], u: &[u8; 32]) -> [u8; 32];
#[must_use] pub fn public_key(scalar: &[u8; 32]) -> [u8; 32];
```

RFC 7748 section 5. The scalar is clamped inside `x25519`; the top bit of `u` is masked per the RFC. `x25519` itself does not reject low-order points (the output is then all zeros); the caller must check if it matters. `age::decrypt` does: it skips an all-zero shared secret. `public_key(scalar)` = `x25519(scalar, &BASEPOINT)`.
