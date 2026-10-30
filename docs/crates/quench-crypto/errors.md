---
title: Errors
parent: quench-crypto
nav_order: 3
---

Every error type implements `std::error::Error` and `Display`. Verification functions that only answer yes/no (`ed25519::verify`, `rsa::verify_pkcs1_sha256`, `hmac::verify`, `pbkdf2::verify`, `ct::eq`, `Totp::check_*`) return `bool`.

| Type | Module | Variants / meaning |
|---|---|---|
| `OpenError` | `aead` | unit struct; any failure to open |
| `Error` | `age` | `Format`, `Header`, `NoMatchingIdentity`, `BadMac`, `Payload`, `NoRecipients` |
| `ParamsError` | `argon2` | parameters or salt outside the valid range |
| `PhcError` | `argon2` | PHC text not usable |
| `KeyError` | `ed25519` | not an Ed25519 PKCS#8 key |
| `TooLong` | `hkdf` | output above 8160 bytes |
| `Error` / `ErrorKind` | `jwt` | token and claim problems |
| `Error` | `totp` | `SecretTooShort`, `BadBase32`, `BadDigits` |

## OpenError

Returned by `ChaCha20Poly1305` and `XChaCha20Poly1305` `decrypt`/`open`. Occurs when the key differs, the associated data differs, the ciphertext or tag was changed, the input is shorter than the tag, or (for `open`) shorter than the nonce. It carries no detail on purpose.

## age::Error

| Variant | Occurs |
|---|---|
| `Format(String)` | `Recipient`/`Identity::from_str` on bad bech32, wrong prefix or length; `dearmor` on bad armor lines or base64; `decrypt` on bad armor |
| `Header(String)` | malformed header (including `unreadable X25519 share`, `an X25519 share is 32 bytes`, `an X25519 stanza wraps a 16-byte key`) |
| `NoMatchingIdentity` | no identity opens any `X25519` stanza (also for an empty identity list, and when only non-X25519 stanzas such as scrypt are present) |
| `BadMac` | the header MAC (HMAC-SHA-256 over the header up to `---`) does not verify |
| `Payload(String)` | `missing nonce`, `no data`, `authentication failed`, `an empty final chunk after data` |
| `NoRecipients` | `encrypt` with an empty slice |

## argon2 errors

### ParamsError

Raised by `hash_raw`, `hash_raw_keyed`, `hash_password`, `hash_password_with_salt`. Texts are listed in the [API reference](api.md#argon2-feature-argon2).

### PhcError

Raised by `verify_password`. Display is `invalid Argon2 hash string: <reason>` with one of:

| Reason | Cause |
|---|---|
| `it must start with `$`` | text does not begin with `$` |
| `only argon2id is supported` | `argon2i`, `argon2d`, others |
| `only version 19 (0x13) is supported` | missing or other `v=` |
| `the parameters are missing` | no parameter field |
| `expected a salt and a hash` | wrong number of `$` parts (also extra parts) |
| `a parameter is not name=value` / `a parameter is not a number` / `unknown parameter` / `a parameter is repeated` | bad parameter list (only `m`, `t`, `p`; each once; `u32`) |
| `the salt is not base64` / `the hash is not base64` | unpadded standard base64 expected |
| `m is missing` / `t is missing` / `p is missing` | a required parameter absent |
| `the parameters are too costly to verify` | `verify_password` only: the stored hash asks for more than 1 GiB of memory, 16 passes or 16 lanes |
| `the parameters are out of range` | the parsed values fail the `ParamsError` checks (for example salt under 8 bytes, hash under 4 bytes) |

## jwt errors

`jwt::Error` wraps an `ErrorKind` (read it with `Error::kind()`).

| `ErrorKind` | Display | Occurs |
|---|---|---|
| `InvalidToken` | `InvalidToken` | not exactly three dot-separated parts; header is not JSON or has no string `alg` |
| `InvalidSignature` | `InvalidSignature` | signature does not verify (also wrong-length signatures) |
| `InvalidAlgorithm` | `InvalidAlgorithm` | header `alg` unknown (`none` included) or not in the allowed list; key family does not match the algorithm; `encode` with `RS256` or a mismatched key |
| `InvalidKeyFormat` | `InvalidKeyFormat` | `EncodingKey::from_ed_der` with a bad PKCS#8 document |
| `ExpiredSignature` | `ExpiredSignature` | `exp < now - leeway` |
| `ImmatureSignature` | `ImmatureSignature` | `nbf > now + leeway` (only with `validate_nbf`) |
| `MissingRequiredClaim(c)` | `Missing required claim: c` | a claim in `required_spec_claims` is absent or `null`; `iss` required by `Validation::iss` but absent; `aud` absent when `Validation::aud` is set |
| `InvalidAudience` | `InvalidAudience` | `validate_aud` and no `aud` match (including `aud = None`) |
| `InvalidIssuer` | `InvalidIssuer` | `iss` present but not in the allowed set, or not a string |
| `Base64(why)` | `Base64 error: why` | a part is not unpadded base64url; JWK `x`, `n`, `e` bad |
| `Json(why)` | `JSON error: why` | payload not JSON; `exp`/`nbf` present but not a number (`` `exp` is not a number ``); claims do not deserialize into `T` |

## totp::Error

| Variant | Display |
|---|---|
| `SecretTooShort(len)` | `a one-time-password secret needs at least 16 bytes, got {len}` |
| `BadBase32(why)` | `the secret is not valid base32: {why}` |
| `BadDigits(n)` | `a code has 6 to 9 digits, not {n}` |

## Others

* `ed25519::KeyError`: `not a valid Ed25519 private key`.
* `hkdf::TooLong`: `HKDF output length is more than 255 * 32 bytes`.
