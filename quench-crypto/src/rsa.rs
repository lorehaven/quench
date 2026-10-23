//! RSA PKCS#1 v1.5 signature verification (RS256), for verifying tokens that
//! other systems signed. Signing is not provided: nothing here holds RSA keys.

use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};

/// Whether `signature` is a valid RSASSA-PKCS1-v1_5 / SHA-256 signature of
/// `message` under the DER-encoded `RSAPublicKey` (2048 to 8192 bits).
#[must_use]
pub fn verify_pkcs1_sha256(public_key_der: &[u8], message: &[u8], signature: &[u8]) -> bool {
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public_key_der)
        .verify(message, signature)
        .is_ok()
}

/// Builds the DER `RSAPublicKey` from the big-endian modulus `n` and public
/// exponent `e` (what a JWK carries).
#[must_use]
pub fn public_key_from_components(n: &[u8], e: &[u8]) -> Vec<u8> {
    let mut body = der_integer(n);
    body.extend(der_integer(e));
    let mut out = vec![0x30];
    out.extend(der_length(body.len()));
    out.extend(body);
    out
}

fn der_integer(value: &[u8]) -> Vec<u8> {
    let start = value
        .iter()
        .position(|&b| b != 0)
        .unwrap_or_else(|| value.len().saturating_sub(1));
    let digits = &value[start..];
    let mut content = Vec::with_capacity(digits.len() + 1);
    // A leading 1 bit would read as negative: prefix a zero byte.
    if digits.first().is_none_or(|&b| b & 0x80 != 0) {
        content.push(0);
    }
    content.extend_from_slice(digits);
    let mut out = vec![0x02];
    out.extend(der_length(content.len()));
    out.extend(content);
    out
}

fn der_length(len: usize) -> Vec<u8> {
    if len < 0x80 {
        return vec![u8::try_from(len).unwrap_or(0)];
    }
    let bytes: Vec<u8> = len
        .to_be_bytes()
        .into_iter()
        .skip_while(|&b| b == 0)
        .collect();
    let mut out = vec![0x80 | u8::try_from(bytes.len()).unwrap_or(0)];
    out.extend(bytes);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // A 2048-bit key made with `openssl genrsa`, and `openssl dgst -sha256 -sign`
    // over "quench rsa test message".
    const N: &str = "de89cbcb15d5f59ba4fd94c1520e5beb4d1ece05a75f0f475a3e4e1c6a356059818b336fcad768ec8083f6b3788a3ba4674713c84aa512ddb2231b34650ed50f5952990c877165f77e7284902104b11075199940a8a126e66287e677184f5593ef7104e3e2a130903aad47f5d6d35c1731a4775b27fbd8c0e5e7dea4bd98324dc1da87f3f5f40cbcdb557793b46dc19005379abdb94a9680750dcf3464a6f9c5a9da211f12fb6477df6fa3ea8b5cf6781d6ff579568e40acffae6d443c20c153656fc863b019f2319fcad6d93cfdaa925677d2fc3ba792961e63a336397eedbe607c8e8e9b6bd9164d7604c1d85d5db53f8d555a934d9788947b8252c2076d7d";
    const SIGNATURE: &str = "561ebe16bb2584597d1e1e41c207dd9720e9c9586b326df680daca83e069e13076a00756aed1e9467fc2069d235560f9927f60d8ef9d4189f4944e91c83896805d24d7926003e6cbc4a1ebd267dcd0c51bd4957b4e24395af45f33ac85ad2390e2fe36f96c81fee49b5d5bee721e237909ac6d00be2217c6dddfd1fff0e3024adb347ebd2a44ebd26d7e62769559086e64e85614b8c991c4dc50a33a2061cb8c3c1f5c5856f81834e8c0d875aca65ff9a35f7b04c533c90414978e0a7ce92d385b4d4199c68566141946e6362f45ebcd31236cf778133e9c434fd575d107cc51f4da5fc2ee383a7ebd87d09f88477e77fb3785445a506500927d17eeee6bfd77";
    const SPKI: &str = "30820122300d06092a864886f70d01010105000382010f003082010a0282010100de89cbcb15d5f59ba4fd94c1520e5beb4d1ece05a75f0f475a3e4e1c6a356059818b336fcad768ec8083f6b3788a3ba4674713c84aa512ddb2231b34650ed50f5952990c877165f77e7284902104b11075199940a8a126e66287e677184f5593ef7104e3e2a130903aad47f5d6d35c1731a4775b27fbd8c0e5e7dea4bd98324dc1da87f3f5f40cbcdb557793b46dc19005379abdb94a9680750dcf3464a6f9c5a9da211f12fb6477df6fa3ea8b5cf6781d6ff579568e40acffae6d443c20c153656fc863b019f2319fcad6d93cfdaa925677d2fc3ba792961e63a336397eedbe607c8e8e9b6bd9164d7604c1d85d5db53f8d555a934d9788947b8252c2076d7d0203010001";
    const MESSAGE: &[u8] = b"quench rsa test message";

    fn key() -> Vec<u8> {
        public_key_from_components(&hex::decode(N).unwrap(), &[0x01, 0x00, 0x01])
    }

    #[test]
    fn verifies_an_openssl_signature() {
        let signature = hex::decode(SIGNATURE).unwrap();
        assert!(verify_pkcs1_sha256(&key(), MESSAGE, &signature));
        assert!(!verify_pkcs1_sha256(&key(), b"another message", &signature));
        let mut flipped = signature.clone();
        flipped[10] ^= 1;
        assert!(!verify_pkcs1_sha256(&key(), MESSAGE, &flipped));
        assert!(!verify_pkcs1_sha256(&key(), MESSAGE, &signature[1..]));
        assert!(!verify_pkcs1_sha256(b"not a key", MESSAGE, &signature));
    }

    #[test]
    fn built_keys_match_the_der_openssl_wrote() {
        // The SubjectPublicKeyInfo ends with the RSAPublicKey we build.
        let spki = hex::decode(SPKI).unwrap();
        let built = key();
        assert!(
            spki.ends_with(&built),
            "{} vs {}",
            hex::encode(&built),
            hex::encode(&spki)
        );
    }

    #[test]
    fn integers_are_minimal_and_non_negative() {
        assert_eq!(der_integer(&[0x00, 0x00, 0x7f]), [0x02, 0x01, 0x7f]);
        assert_eq!(der_integer(&[0x80]), [0x02, 0x02, 0x00, 0x80]);
        assert_eq!(der_integer(&[0x00]), [0x02, 0x01, 0x00]);
        assert_eq!(der_integer(&[]), [0x02, 0x01, 0x00]);
        assert_eq!(
            der_integer(&[0x01, 0x00, 0x01]),
            [0x02, 0x03, 0x01, 0x00, 0x01]
        );
    }

    #[test]
    fn lengths_use_short_and_long_forms() {
        assert_eq!(der_length(0), [0x00]);
        assert_eq!(der_length(0x7f), [0x7f]);
        assert_eq!(der_length(0x80), [0x81, 0x80]);
        assert_eq!(der_length(0x100), [0x82, 0x01, 0x00]);
        assert_eq!(der_length(0x1_0000), [0x83, 0x01, 0x00, 0x00]);
    }
}
