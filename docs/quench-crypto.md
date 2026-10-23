# Quench Crypto

`quench-crypto` (crate `quench_crypto`) holds the cryptography quench services need, in one place, on top of [`ring`](https://github.com/briansmith/ring) (already in the build through `rustls`, and the only crypto backend allowed in this workspace). It replaces `sha2`, `argon2`, `ed25519-dalek`, `jsonwebtoken` and the constant-time/PBKDF2 helpers that other crates would otherwise pull in. Nothing here implements a cipher or a signature scheme from scratch except Argon2id (with BLAKE2b), which `ring` does not offer and which has an official test vector.

## Features

`hash`, `hmac` (SHA-1 too, for protocols that fix it), `ct` and `pbkdf2` are always built. The rest sit behind features, all on by default:

| Feature | Module | Notes |
| --- | --- | --- |
| `argon2` | `argon2` | includes the BLAKE2b it is built on |
| `ed25519` | `ed25519` | |
| `rsa` | `rsa` | PKCS#1 v1.5 verification only |
| `totp` | `totp` | HOTP and TOTP |
| `aead` | `aead` | ChaCha20-Poly1305 and XChaCha20-Poly1305 |
| `x25519` | `x25519` | X25519 key agreement (RFC 7748) for stored key pairs |
| `age` | `age` | age v1 file encryption to X25519 recipients; enables `aead` and `x25519` |
| `jwt` | `jwt` | enables `ed25519` and `rsa`, and brings in `quench-serde` |

`quench-pg`, `quench-db` and `quench-proto` use the always-built modules only; `quench-auth` asks for `argon2` and `jwt`.

## Modules

- **`hash`** — `sha256`, `sha384`, `sha512`, a generic `digest(Algorithm, ..)` and an incremental `Hasher`.
- **`hmac`** — `sign` / `verify` (constant-time) with SHA-256/384/512.
- **`ct`** — `eq(a, b)`, constant-time comparison for secrets, MACs and tokens.
- **`pbkdf2`** — `derive` / `verify` with HMAC-SHA-2.
- **`ed25519`** — `SigningKey` (from a seed, generated, or read from PKCS#8 v1 or v2 DER), `sign`, `public_key`, and `verify`. `to_pkcs8_der` writes `ed25519-dalek`'s v2 layout byte for byte, so stored keys stay readable by builds that still use it. `Debug` never prints the seed.
- **`rsa`** — `verify_pkcs1_sha256` and `public_key_from_components(n, e)` (a JWK's modulus and exponent to DER). Verify only: nothing here holds RSA private keys.
- **`argon2`** — Argon2id v0x13 (RFC 9106). `hash_password` (fresh 16-byte salt) and `verify_password` read and write the PHC string `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`; `Params::default()` is the `argon2` crate's default, so every hash stored so far keeps verifying and new ones are identical in form. `hash_raw` / `hash_raw_keyed` give the raw tag (with the optional secret and associated data). A malformed hash string is an error, a wrong password is `Ok(false)`.
- **`totp`** — one-time passwords as authenticator apps use them. `hotp(algorithm, key, counter, digits)` (RFC 4226) and `Totp` (RFC 6238): `Totp::new(secret)` / `from_base32(text)` (at least 16 bytes) with SHA-1 (the apps' default), 6 digits, a 30 s step and one step of skew, changed with `with_algorithm` (SHA-1/256/512), `with_digits`, `with_step`, `with_skew`. `code_at(unix_secs)` / `code_now()` make a code; `check_at(code, unix_secs)` / `check_now(code)` accept the current step and `skew` steps either side, comparing every window in constant time and ignoring surrounding whitespace. `generate_secret()` is 20 random bytes, `secret_base32()` the text to type in by hand, and `url(issuer, account)` the `otpauth://totp/Issuer:account?secret=..&issuer=..&algorithm=SHA1&digits=6&period=30` URL for a QR code (see [Quench QR](./quench-qr.md)). `Debug` never prints the secret.
- **`aead`** — `ChaCha20Poly1305` (RFC 8439, 12-byte nonce) and `XChaCha20Poly1305` (24-byte nonce, safe to pick at random). `encrypt` / `decrypt` take the nonce and the associated data explicitly and return `ciphertext || tag`; `seal` / `open` choose a random nonce and keep it in front (`nonce || ciphertext || tag`), which is what storage usually wants. The associated data is bound in: a ciphertext opened under other data, another key, or after any change fails with `OpenError`, which deliberately says nothing about why. XChaCha20 is HChaCha20 (written here) deriving a sub-key, then ChaCha20-Poly1305 from `ring`. The wire format is the same as the `chacha20poly1305` crate's, so values sealed by it open here and the other way round. `Debug` never prints the key.
- **`hkdf`** — `sha256(ikm, salt, info, length)`, HKDF with HMAC-SHA-256 (RFC 5869), always built.
- **`x25519`** — `x25519(scalar, u)` and `public_key(scalar)`: the RFC 7748 scalar multiplication on five 51-bit limbs with a constant-time ladder. `ring` only offers X25519 with single-use ephemeral keys, so a key that is stored and reloaded needs this. Checked against the RFC's vectors, including the iterated 1 and 1000 round ones.
- **`age`** — [age](https://age-encryption.org/v1) files to X25519 recipients, compatible both ways with the `age` tool. `Identity` (`AGE-SECRET-KEY-1...`: `generate`, `FromStr`, `to_public`, `to_secret_string`; `Debug` never prints it), `Recipient` (`age1...`), `encrypt(&[Recipient], plaintext)`, `decrypt(&[Identity], data)` (binary or armored; the first identity that fits, header MAC and every chunk checked), `armor` / `dearmor`. Not supported: passphrase (scrypt) and plugin recipients. Interop was checked against the `age` crate on 300 random files in both directions, sized around the 64 KiB chunk boundaries.
- **`jwt`** — `encode`, `decode`, `decode_header`, `verify` over compact JWS. `EdDSA`, `HS256/384/512` and (verify only) `RS256`. Defaults follow `jsonwebtoken`: `exp` required and checked with a 60 s leeway, `nbf` only on request, the header algorithm must be one the caller allows, and the key must belong to that algorithm's family (an HMAC secret never verifies an EdDSA token; `alg: none` is refused). Errors carry an `ErrorKind` (`InvalidSignature`, `ExpiredSignature`, ...) so callers can still branch on the reason. `DecodingKey::from_ed_components` / `from_rsa_components` build keys from JWK fields.

## Verification

- RFC test vectors: HOTP (RFC 4226 appendix D), TOTP (RFC 6238 appendix B, all three hashes), ChaCha20-Poly1305 (RFC 8439 section 2.8.2), HChaCha20 and XChaCha20-Poly1305 (draft-irtf-cfrg-xchacha), SHA-2 (FIPS 180), HMAC (RFC 4231), PBKDF2 (RFC 7914), Ed25519 (RFC 8032), JWS examples (RFC 7515 A.1, RFC 8037 A.4), BLAKE2b (RFC 7693), Argon2id (RFC 9106 section 5.3, 4 lanes with secret and associated data), and an OpenSSL-made RSA signature.
- Against the crates this replaces: 3,000 random Argon2id inputs (1–4 lanes, assorted memory, passes and output lengths) match the `argon2` crate bit for bit, hash strings cross-verify in both directions, and the default-parameter PHC text is identical; EdDSA tokens are byte-identical to `jsonwebtoken`'s for the same key and claims, every `exp` boundary and tampering case behaves the same, and HS256 tokens interoperate both ways.

Argon2 is deliberately slow and memory-hungry (19 MiB by default); the workspace builds this crate with optimisations even in the dev profile so tests that hash stay quick.

[Home](../README.md)
