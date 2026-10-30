---
title: Testing
parent: quench-crypto
nav_order: 5
---

* The crate uses `quench-test`'s own harness: `[lib] harness = false` and `quench_test::main!();` in `lib.rs`; tests are `#[quench_test::test]` functions inside each module's `#[cfg(test)] mod tests`.
* One integration test, `tests/security_jwt.rs` (`required-features = ["jwt"]`, `harness = false`), builds hostile tokens by hand and checks: `alg: none` in every spelling, HMAC-with-public-key confusion, algorithm not allowed by the caller, malformed structure, every single-bit flip of a valid token, non-canonical Ed25519 signatures, expiry and bounded leeway, missing `exp`, wrong-typed `exp`/`nbf`, audience and issuer exact matching, duplicate claim names, deeply nested header/claims JSON, oversized tokens, `kid` values that must not redirect key lookup.
* Module tests cite these references: RFC 4231 (HMAC), RFC 6238 (TOTP), RFC 8032 (Ed25519), RFC 7693 (BLAKE2b), RFC 8439 (ChaCha20-Poly1305), RFC 7748 (X25519), RFC 4226 (HOTP), RFC 7515 and RFC 8037 (JWS/EdDSA), RFC 7914 (PBKDF2), the XChaCha20 draft, RFC 9106 section 5.3 and hashes made by the `argon2` crate (Argon2id), plus age round trips, tamper and malformed-header cases.
* Running: `cargo test -p quench-crypto` (add `--no-default-features --features ...` to test a feature subset).
* To test code built on it: fixed seeds make Ed25519 keys deterministic (`SigningKey::from_seed(&[7u8; 32])`); `Totp::code_at` / `check_at` take the time explicitly; `argon2::hash_password_with_salt` gives reproducible hashes; use small `Params` (for example `memory_kib: 8, iterations: 1, lanes: 1, output_len: 32`) to keep tests fast.
