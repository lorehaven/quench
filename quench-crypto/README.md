# quench-crypto
Hashing, HMAC, Ed25519, RSA verification, PBKDF2, Argon2id and JWT on ring.

## What it is
`quench-crypto` is the cryptography of the Quench estate: hashes, MACs, key derivation, password hashing, signatures, authenticated encryption, one-time passwords, `age` file encryption and JSON Web Tokens. It sits on `ring` and replaces `sha2`, `argon2`, `ed25519-dalek`, `jsonwebtoken`, `chacha20poly1305`, `age` and the constant-time helpers.

Argon2id, BLAKE2b, HChaCha20, X25519, age, HOTP/TOTP and JWT are written in the crate; the rest is `ring`. It depends on `quench-core` (randomness and encodings) and, for JWT, `quench-serde`. The framework consumer is `quench-auth`; QR codes for TOTP URLs come from `quench-qr`.

## Use cases
- Hash and verify passwords with Argon2id.
- Sign and verify messages with Ed25519.
- Issue and validate JWTs.
- Seal data at rest with XChaCha20-Poly1305.
- Add TOTP two-factor codes, or compare secrets in constant time.

## Example
```rust
use quench_crypto::argon2::{self, Params};

let stored: String = argon2::hash_password("correct horse", &Params::default())?;
assert_eq!(argon2::verify_password("correct horse", &stored), Ok(true));
assert_eq!(argon2::verify_password("wrong", &stored), Ok(false));
# Ok::<(), argon2::ParamsError>(())
```

```rust
use quench_crypto::ed25519::{self, SigningKey};

let key = SigningKey::generate();
let sig: [u8; 64] = key.sign(b"message");
assert!(ed25519::verify(&key.public_key(), b"message", &sig));
```

## Features
`hash`, `hmac`, `hkdf`, `ct` and `pbkdf2` are always built; the rest are on by default.
- `argon2`: Argon2id password hashing.
- `ed25519`: signing keys and verification.
- `rsa`: RSA PKCS#1 v1.5 verification.
- `totp`: HOTP and TOTP with `otpauth://` URLs.
- `aead`: ChaCha20-Poly1305 and XChaCha20-Poly1305.
- `x25519`: key agreement.
- `age`: age file encryption (implies `aead`, `x25519`).
- `jwt`: JSON Web Tokens (implies `ed25519`, `rsa`).

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-crypto/
