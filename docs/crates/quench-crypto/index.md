---
title: quench-crypto
parent: Crates
nav_order: 14
has_children: true
permalink: /crates/quench-crypto/
---

`quench-crypto` (crate `quench_crypto`) is the cryptography of the Quench estate: hashes, MACs, key derivation, password hashing, signatures, authenticated encryption, one-time passwords, `age` file encryption and JSON Web Tokens. It sits on [`ring`](https://github.com/briansmith/ring) (the one crypto backend allowed in the workspace; `rustls` already uses it) and replaces `sha2`, `argon2`, `ed25519-dalek`, `jsonwebtoken`, `chacha20poly1305`, `age` and the constant-time helpers.

* Version: `0.1.8`. Edition 2024 (no `rust-version` is declared; the sources use let-chains, so a toolchain that supports edition 2024 and `if let ... &&` chains is needed).
* No `unsafe` code in the crate (`unsafe_code = "deny"` through `[lints] workspace = true`; grep finds none).
* Primitives written in this crate rather than taken from `ring`: Argon2id and BLAKE2b, HChaCha20 (for XChaCha20-Poly1305), X25519 scalar multiplication, age (header and payload framing, bech32), HOTP/TOTP, JWT. Everything else (SHA-1/2, HMAC, PBKDF2, Ed25519, RSA verify, ChaCha20-Poly1305) is `ring`.

## Install

```toml
[dependencies]
quench-crypto = { version = "0.1", registry = "ennor" }
# only what you need:
# quench-crypto = { version = "0.1", registry = "ennor", default-features = false, features = ["ed25519", "jwt"] }
```

(`registry = "ennor"` is how the workspace itself depends on its crates; inside the workspace use `quench-crypto = { workspace = true }`.)

## Feature flags

`hash`, `hmac`, `hkdf`, `ct` and `pbkdf2` are always built. Everything else is behind a feature; **all features are on by default** (`default = ["argon2", "ed25519", "rsa", "jwt", "totp", "aead", "x25519", "age"]`).

| Feature | Enables | Implies | Extra dependency |
|---|---|---|---|
| `argon2` | `argon2` (and the private `blake2b` module) | | |
| `ed25519` | `ed25519` | | |
| `rsa` | `rsa` | | |
| `totp` | `totp` | | |
| `aead` | `aead` | | |
| `x25519` | `x25519` | | |
| `age` | `age` | `aead`, `x25519` | |
| `jwt` | `jwt` | `ed25519`, `rsa` | `quench-serde` |

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | `random` (OS randomness for salts and nonces), `base32`, `base64`, `hex`, `time` |
| `ring` (0.17, `alloc` + `std`, no default features) | SHA-1/2, HMAC, PBKDF2, Ed25519, RSA PKCS#1 verify, ChaCha20-Poly1305, HKDF |
| `quench-serde` (optional, feature `jwt`) | JSON values and (de)serialization of claims |
| `quench-test` (dev) | test harness (`harness = false`) |

## Where to go next

* [Usage](usage.md): worked examples (password hashing, signing, sealing, JWT, TOTP, age).
* [API reference](api.md): every public item.
* [Errors](errors.md): error types and when they occur.
* [Internals](internals.md): algorithms, parameters, byte layouts, security properties and what is *not* guaranteed.
* [Testing](testing.md).
* The consumer of these primitives in the framework is `quench-auth`; QR codes for `Totp::url` come from [quench-qr](../quench-qr/index.md).

## Minimal example

```rust
use quench_crypto::{argon2, hash, hmac};

let digest = hash::sha256(b"abc");
assert_eq!(digest.len(), 32);

let tag = hmac::sign(hmac::Algorithm::Sha256, b"key", b"message");
assert!(hmac::verify(hmac::Algorithm::Sha256, b"key", b"message", &tag));

let phc = argon2::hash_password("hunter2", &argon2::Params::default())?;
assert!(argon2::verify_password("hunter2", &phc)?);
# Ok::<(), Box<dyn std::error::Error>>(())
```
