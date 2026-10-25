#![no_main]
use libfuzzer_sys::fuzz_target;
use quench_crypto::ed25519::SigningKey;
use quench_crypto::jwt::{Algorithm, DecodingKey, Validation, decode, decode_header};
use quench_serde::Value;

// The token is attacker-controlled and its header is parsed before the signature is checked. The
// signing key is fixed and unknown to the fuzzer, so no input should ever verify.
fuzz_target!(|data: &[u8]| {
    let Ok(token) = std::str::from_utf8(data) else {
        return;
    };
    let _ = decode_header(token);
    let key = DecodingKey::from_ed_der(&SigningKey::from_seed(&[7u8; 32]).public_key());
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_aud = false;
    assert!(
        decode::<Value>(token, &key, &validation).is_err(),
        "forged token verified"
    );
});
