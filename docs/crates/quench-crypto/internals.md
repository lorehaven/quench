---
title: Internals
parent: quench-crypto
nav_order: 4
---

Algorithms, parameters, byte layouts and security properties, each checked against the source.

## What comes from ring and what is written here

| Function | Implementation |
|---|---|
| SHA-1/256/384/512, HMAC, PBKDF2, HKDF-SHA-256 | `ring` |
| Ed25519 sign/verify, RSA PKCS#1 v1.5 verify (SHA-256, 2048-8192 bit), ChaCha20-Poly1305 | `ring` |
| BLAKE2b (RFC 7693) and Argon2id (RFC 9106) | this crate (`blake2b.rs`, `argon2.rs`) |
| HChaCha20 / XChaCha20-Poly1305 | this crate (HChaCha20) on `ring`'s ChaCha20-Poly1305 |
| X25519 scalar multiplication | this crate (`x25519.rs`) |
| age, bech32 | this crate (`age.rs`) |
| HOTP/TOTP, JWT | this crate |

## Constant-time claims

Only these are claimed by the source:

* `ct::eq`: all bytes of equal-length inputs are examined and the OR-ed difference is checked at the end, behind `std::hint::black_box`. Lengths are compared first and are public. There is no assembly-level guarantee; it is a best-effort source-level pattern.
* `x25519`: "a Montgomery ladder whose control flow and memory access do not depend on the secret scalar" (module documentation); the swap is done with `cswap`. Five 51-bit limbs.
* `Totp::check_at`: all `2*skew+1` windows are computed and compared with `ct::eq`, with no early exit.
* `hmac::verify`, `pbkdf2::verify`: `ring`'s constant-time verification. `jwt` HMAC signature comparison and `argon2::verify_password` use `ct::eq`.
* Argon2id's data-dependent addressing is inherent to the algorithm (the `id` variant is data-independent in the first half of the first pass only); nothing more is claimed.
* Keys and secrets are **not zeroized** on drop anywhere in the crate. `Debug` implementations of `SigningKey`, `ChaCha20Poly1305`, `XChaCha20Poly1305`, `Identity`, `EncodingKey`, `DecodingKey` and `Totp` do not print key material.

## Argon2id

* Version 0x13, type `id` (2) only. PHC format: `$argon2id$v=19$m=<KiB>,t=<passes>,p=<lanes>$<salt>$<hash>`, unpadded standard base64 for salt and hash.
* Seed hash H0 = BLAKE2b-64 over `lanes, output_len, memory_kib, iterations, 0x13, 2` (each u32 LE) followed by length-prefixed (u32 LE) password, salt, secret, associated data. 4 sync points per pass. Lanes are computed sequentially (no threads); the result does not depend on that.
* Memory actually allocated: `4 * lanes * floor(memory_kib / (4 * lanes))` blocks of 1 KiB (128 u64 words).
* **Resource note:** `verify_password` takes `m`, `t`, `p` from the stored text. `m` can be up to `u32::MAX` KiB (about 4 TiB) and `t` up to `u32::MAX`; the code has no upper bound check. Only verify hashes you stored yourself.
* Default `Params`: 19456 KiB, 2 passes, 1 lane, 32-byte output (the OWASP minimum for Argon2id).
* Tested against the RFC 9106 section 5.3 vector (4 lanes, secret and associated data) and against hashes made by the `argon2` crate.

## Ed25519 PKCS#8 layout

Seed offset in both versions is bytes 16..48.

```text
v1 (48 bytes, read only):
  30 2e 02 01 00 30 05 06 03 2b 65 70 04 22 04 20 | seed (32)

v2 (83 bytes, written; read):
  30 51 02 01 01 30 05 06 03 2b 65 70 04 22 04 20 | seed (32) | 81 21 00 | public key (32)
```

`to_pkcs8_der` writes v2 byte for byte as `ed25519-dalek` does. `from_pkcs8_der` accepts what `ring` accepts under `Ed25519KeyPair::from_pkcs8_maybe_unchecked`.

## AEAD

* ChaCha20-Poly1305: 32-byte key, 12-byte nonce, 16-byte tag, `ring`'s `LessSafeKey`.
* XChaCha20-Poly1305: subkey = HChaCha20(key, nonce[0..16]); inner nonce = 4 zero bytes || nonce[16..24]; then ChaCha20-Poly1305. The wire format equals the `chacha20poly1305` crate's, per the old documentation and the test vectors of draft-irtf-cfrg-xchacha.
* `seal` layout: `nonce (12 or 24) || ciphertext || tag (16)`. Nonces come from the operating system's randomness (`quench_core::random`). Nonce reuse protection beyond that is the caller's job for `encrypt`.

## age

Implemented subset of `age-encryption.org/v1`:

* Recipients: `X25519` stanzas only (`age1...` keys, `AGE-SECRET-KEY-1...` identities; bech32 with the HRPs `age` and `age-secret-key-`). Passphrase (scrypt) and plugin recipients are **not** supported; such stanzas are skipped on decrypt.
* Header text: `age-encryption.org/v1\n`, per recipient `-> X25519 <base64 share>\n<wrapped key, base64 wrapped at 64 columns>`, then `--- <base64 MAC>\n`. MAC = HMAC-SHA-256 with key derived from the 16-byte file key (`header_mac_key`) over the header bytes up to and including `---`.
* Payload: 16-byte random nonce, then ChaCha20-Poly1305 chunks of 64 KiB (65 536) plus 16-byte tag; the final chunk is flagged in its nonce; an empty plaintext is a single empty final chunk; an empty final chunk after data is rejected; trailing data after the END line of armor is rejected.
* Interop (from the source's own documentation): both directions with the `age` tool.

## JWT

* Format: unpadded base64url of header, payload, signature joined by `.`.
* Accepted header `alg`: `EdDSA`, `HS256`, `HS384`, `HS512`, `RS256`. `none` and any other name is `InvalidAlgorithm`.
* **Algorithm pinning:** the header algorithm must be in the caller's list *and* the key variant must belong to the algorithm's family (`EdDSA` with `DecodingKey::Ed25519`, `RS256` with `Rsa`, `HS*` with `Hmac`). Otherwise `InvalidAlgorithm`.
* Claim checks in `decode` (run after the signature):
  1. every name in `required_spec_claims` must be present and not `null` (else `MissingRequiredClaim`);
  2. `exp` (when `validate_exp`): present and a number (fractional values are floored; a non-number gives `Json`), `exp < now - leeway` is `ExpiredSignature`;
  3. `nbf` (when `validate_nbf`): same rules, `nbf > now + leeway` is `ImmatureSignature`;
  4. `iss` (when `iss` is `Some`): must be a string in the set, absent is `MissingRequiredClaim("iss")`;
  5. `aud` (when `validate_aud`): a string or an array of strings; `aud = None` always yields `InvalidAudience`; `aud = Some(set)` with no `aud` claim yields `MissingRequiredClaim("aud")`.
* Duplicate JSON object keys resolve to the last value (tested in `security_jwt.rs`); deeply nested JSON in header or claims is refused without stack overflow (`quench_serde::json::MAX_DEPTH` is 128).
* With the default `Validation::new`, `exp` is not required to be in the future by more than `leeway` seconds; `iat` is never checked.

## HOTP/TOTP

RFC 4226 dynamic truncation (`offset = last byte & 0x0f`, 31-bit word, modulo 10^digits, digits clamped to 9), TOTP counter = `unix_seconds / step`. Secrets: at least 16 bytes; `generate_secret` makes 20. `otpauth://` URL format in the [API reference](api.md#totp-feature-totp).

## Not provided

RSA signing, ECDSA, AES-GCM, scrypt, SHA-3, constant-time bignum code, key zeroization, rate limiting or replay protection for TOTP, JWK set parsing, JWE.
