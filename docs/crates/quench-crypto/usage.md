---
title: Usage
parent: quench-crypto
nav_order: 1
---

Worked examples, from the common to the advanced. Every function used here exists with exactly this signature (see the [API reference](api.md)).

## 1. Hash data, incrementally or at once

```rust
use quench_crypto::hash::{self, Algorithm, Hasher};

let one_shot = hash::sha256(b"hello world");           // [u8; 32]
let generic = hash::digest(Algorithm::Sha512, b"abc");  // Vec<u8>, 64 bytes

let mut h = Hasher::new(Algorithm::Sha256);
h.update(b"hello ");
h.update(b"world");
assert_eq!(h.finish(), one_shot.to_vec());
```

Pitfall: `sha1_legacy` exists only for protocols that mandate SHA-1 (the WebSocket accept key). Do not use it for signatures or new designs.

## 2. MAC a message and compare in constant time

```rust
use quench_crypto::{ct, hmac};

let key = b"a secret of any length";
let tag = hmac::sign(hmac::Algorithm::Sha256, key, b"payload");
assert!(hmac::verify(hmac::Algorithm::Sha256, key, b"payload", &tag));
assert!(ct::eq(&tag, &tag));
```

Never compare MACs, tokens or secrets with `==`; use `ct::eq` or the `verify` functions, which use it.

## 3. Hash and verify a password (Argon2id)

```rust
use quench_crypto::argon2::{self, Params};

let params = Params::default(); // 19 MiB, 2 passes, 1 lane, 32-byte hash
let stored: String = argon2::hash_password("correct horse", &params)?;
// stored looks like: $argon2id$v=19$m=19456,t=2,p=1$<salt b64>$<hash b64>

assert_eq!(argon2::verify_password("correct horse", &stored), Ok(true));
assert_eq!(argon2::verify_password("wrong", &stored), Ok(false)); // wrong password: Ok(false)
assert!(argon2::verify_password("x", "not a hash").is_err());      // unreadable hash: Err(PhcError)
# Ok::<(), argon2::ParamsError>(())
```

The cost parameters are read from the stored string, so old hashes keep verifying when you change `Params`. Pitfall: because they are read from the string, never call `verify_password` on a hash string supplied by an untrusted party: `m=` can ask for up to 4 TiB of memory (see [internals](internals.md#argon2id)).

## 4. Derive keys (HKDF, PBKDF2)

```rust
use quench_crypto::{hkdf, hmac::Algorithm, pbkdf2};

let okm = hkdf::sha256(b"input key material", b"salt", b"app v1 encryption", 32)?;
assert_eq!(okm.len(), 32);

let mut key = [0u8; 32];
pbkdf2::derive(Algorithm::Sha256, 600_000, b"salt", b"password", &mut key);
assert!(pbkdf2::verify(Algorithm::Sha256, 600_000, b"salt", b"password", &key));
# Ok::<(), quench_crypto::hkdf::TooLong>(())
```

## 5. Sign and verify with Ed25519

```rust
use quench_crypto::ed25519::{self, SigningKey};

let key = SigningKey::generate();
let sig: [u8; 64] = key.sign(b"message");
assert!(ed25519::verify(&key.public_key(), b"message", &sig));

// persist and reload (PKCS#8 v2, 83 bytes)
let der = key.to_pkcs8_der();
let again = SigningKey::from_pkcs8_der(&der).unwrap();
assert_eq!(again.public_key(), key.public_key());
```

## 6. Seal data at rest (XChaCha20-Poly1305)

```rust
use quench_crypto::aead::XChaCha20Poly1305;

let cipher = XChaCha20Poly1305::new([7u8; 32]);
let sealed = cipher.seal(b"row:42", b"secret value");   // nonce(24) || ciphertext || tag(16)
assert_eq!(cipher.open(b"row:42", &sealed).unwrap(), b"secret value");
assert!(cipher.open(b"row:43", &sealed).is_err());      // other associated data: OpenError
```

Use the 12-byte `ChaCha20Poly1305` only when the nonce comes from a counter you control; its random nonce is too short to pick at random for a long-lived key (the module documentation says so).

## 7. Issue and verify a JWT

```rust
use quench_crypto::ed25519::SigningKey;
use quench_crypto::jwt::{self, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use quench_serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Claims { sub: String, exp: i64 }

let key = SigningKey::generate();
let enc = EncodingKey::from_ed_key(SigningKey::from_seed(key.seed()));
let token = jwt::encode(&Header::new(Algorithm::EdDSA), &Claims { sub: "ada".into(), exp: 4_102_444_800 }, &enc)?;

let mut validation = Validation::new(Algorithm::EdDSA);
validation.validate_aud = false; // default is true: see the pitfall below
let data = jwt::decode::<Claims>(&token, &DecodingKey::from_ed_der(&key.public_key()), &validation)?;
assert_eq!(data.claims.sub, "ada");
# Ok::<(), jwt::Error>(())
```

Pitfall: `Validation::new` sets `validate_aud = true` with `aud = None`. In that state every token is rejected with `InvalidAudience`, whatever its `aud` claim (see [internals](internals.md#jwt)). Either set `validation.aud = Some(...)` or turn `validate_aud` off. The test suite does the same (`security_jwt.rs`, `validation()`).

## 8. One-time passwords (TOTP)

```rust
use quench_crypto::totp::{self, Totp};

let secret = totp::generate_secret();           // 20 random bytes
let t = Totp::new(secret)?;                     // SHA-1, 6 digits, 30 s, skew 1
let url = t.url("Example", "ada@example.com");  // otpauth://totp/Example:ada%40example.com?secret=...
let code = t.code_now();
assert!(t.check_now(&code));
# Ok::<(), totp::Error>(())
```

Render `url` as a QR code with [quench-qr](../quench-qr/index.md). Replay protection (refusing a code twice) is not provided by this crate.

## 9. Encrypt a file to age recipients

```rust
use quench_crypto::age::{self, Identity};

let id = Identity::generate();
let recipient = id.to_public();                    // age1...
let file = age::encrypt(&[recipient], b"plaintext")?;
assert_eq!(age::decrypt(&[id], &file)?, b"plaintext");

let text = age::armor(&file);                      // -----BEGIN AGE ENCRYPTED FILE-----
assert_eq!(age::dearmor(&text)?, file);
# Ok::<(), age::Error>(())
```
