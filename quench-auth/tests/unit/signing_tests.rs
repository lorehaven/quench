//! Key generation and the key shapes gatehouse persists.

use quench_auth::domain::signing::{decoding_key, encoding_key, generate_signing_key};
use quench_crypto::jwt::{Algorithm, Header, Validation, decode, encode};
use quench_serde::json;

#[test]
fn generated_keys_sign_and_verify() {
    let generated = generate_signing_key();
    assert_eq!(generated.public_key.len(), 32);
    assert!(
        format!("{generated:?}").contains("redacted")
            && !format!("{generated:?}").contains(&format!("{:?}", generated.private_key_der))
    );
    let signer = encoding_key(&generated.private_key_der).expect("the DER it wrote is readable");
    let claims = json!({"sub": "alice", "exp": quench_core::time::Time::now().timestamp() + 600});
    let token = encode(&Header::new(Algorithm::EdDSA), &claims, &signer).unwrap();
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.validate_aud = false;
    let verified: quench_serde::Value =
        decode(&token, &decoding_key(&generated.public_key), &validation)
            .unwrap()
            .claims;
    assert_eq!(verified["sub"], "alice");

    let other = generate_signing_key();
    assert!(
        decode::<quench_serde::Value>(&token, &decoding_key(&other.public_key), &validation)
            .is_err()
    );
}

#[test]
fn unreadable_private_keys_are_errors() {
    assert!(encoding_key(b"not a key").is_err());
    assert!(encoding_key(&[]).is_err());
}
