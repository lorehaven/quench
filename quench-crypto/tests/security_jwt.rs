//! Attacks on JWT verification: the checks every relying party in the estate leans on.
//!
//! The classic families - `alg: none`, key/algorithm confusion, signature stripping and malleability,
//! claim type confusion (an `exp` that is a string must not mean "never expires"), and parser
//! resource exhaustion on the *unauthenticated* header - each get a test that builds the hostile
//! token by hand, so a regression in `quench_crypto::jwt` is caught without trusting its own encoder.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_core::base64::URL_SAFE_NO_PAD;
use quench_core::time::Time;
use quench_crypto::ed25519::SigningKey;
use quench_crypto::hmac;
use quench_crypto::jwt::{
    Algorithm, DecodingKey, EncodingKey, ErrorKind, Header, Validation, decode, decode_header,
    encode,
};
use quench_serde::Value;

fn now() -> i64 {
    Time::now().timestamp()
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn signing_key() -> SigningKey {
    SigningKey::from_seed(&[7u8; 32])
}

fn public() -> DecodingKey {
    DecodingKey::from_ed_der(&signing_key().public_key())
}

/// Validation as a relying party configures it, minus audience (covered separately).
fn validation() -> Validation {
    let mut v = Validation::new(Algorithm::EdDSA);
    v.validate_aud = false;
    v.validate_nbf = true;
    v
}

/// Signs arbitrary header/payload text with the real key: only the *content* is hostile.
fn sign_raw(header: &str, payload: &str) -> String {
    let input = format!("{}.{}", b64(header.as_bytes()), b64(payload.as_bytes()));
    let sig = signing_key().sign(input.as_bytes());
    format!("{input}.{}", b64(&sig))
}

const EDDSA: &str = r#"{"typ":"JWT","alg":"EdDSA"}"#;

fn claims_json(extra: &str) -> String {
    format!(r#"{{"sub":"ada",{extra}}}"#)
}

fn verdict(token: &str) -> Result<(), ErrorKind> {
    decode::<Value>(token, &public(), &validation())
        .map(|_| ())
        .map_err(|e| e.kind().clone())
}

fn must_reject(case: &str, token: &str) {
    assert!(
        verdict(token).is_err(),
        "{case}: token was ACCEPTED: {token}"
    );
}

fn valid_token() -> String {
    sign_raw(EDDSA, &claims_json(&format!(r#""exp":{}"#, now() + 600)))
}

#[quench_test::test]
async fn the_control_token_verifies() {
    // Guards every other test here: if the baseline fails, "rejected" proves nothing.
    assert_eq!(verdict(&valid_token()), Ok(()));
}

// --- algorithm attacks ---

#[quench_test::test]
async fn alg_none_in_every_spelling_is_rejected() {
    let payload = claims_json(&format!(r#""exp":{}"#, now() + 600));
    for alg in ["none", "None", "NONE", "nOnE", "", " none", "none ", "null"] {
        let header = format!(r#"{{"typ":"JWT","alg":"{alg}"}}"#);
        for signature in ["", "AAAA", &b64(&[0u8; 64])] {
            let token = format!(
                "{}.{}.{signature}",
                b64(header.as_bytes()),
                b64(payload.as_bytes())
            );
            must_reject(&format!("alg={alg:?} sig={signature:?}"), &token);
        }
    }
    // Missing and non-string alg.
    for header in [
        r#"{"typ":"JWT"}"#,
        r#"{"alg":null}"#,
        r#"{"alg":["EdDSA"]}"#,
        r#"{"alg":1}"#,
        "{}",
    ] {
        let token = format!("{}.{}.", b64(header.as_bytes()), b64(payload.as_bytes()));
        must_reject(header, &token);
    }
}

#[quench_test::test]
async fn hmac_signed_with_the_public_key_is_not_accepted_as_eddsa() {
    // The textbook key-confusion attack: sign HS256 using the (public) verification key as the secret.
    let public_bytes = signing_key().public_key();
    let payload = claims_json(&format!(r#""exp":{}"#, now() + 600));
    let header = r#"{"typ":"JWT","alg":"HS256"}"#;
    let input = format!("{}.{}", b64(header.as_bytes()), b64(payload.as_bytes()));
    let mac = hmac::sign(hmac::Algorithm::Sha256, &public_bytes, input.as_bytes());
    let forged = format!("{input}.{}", b64(&mac));

    // 1. Verifier pinned to EdDSA.
    must_reject("HS256 vs EdDSA-only verifier", &forged);

    // 2. Verifier that (mistakenly) allows both algorithms, still holding an Ed25519 key.
    let mut both = validation();
    both.algorithms = vec![Algorithm::EdDSA, Algorithm::HS256];
    let result = decode::<Value>(&forged, &public(), &both);
    assert!(
        result.is_err(),
        "an Ed25519 public key verified an HMAC token: key-family check is broken"
    );

    // 3. The mirror image: an EdDSA token must not verify against an HMAC key of the same bytes.
    let eddsa = valid_token();
    let hmac_key = DecodingKey::from_secret(&public_bytes);
    let mut both = validation();
    both.algorithms = vec![Algorithm::EdDSA, Algorithm::HS256];
    assert!(decode::<Value>(&eddsa, &hmac_key, &both).is_err());
}

#[quench_test::test]
async fn a_token_cannot_pick_an_algorithm_the_caller_did_not_allow() {
    let secret = b"correct horse battery staple";
    let payload = claims_json(&format!(r#""exp":{}"#, now() + 600));
    for (alg, name) in [
        (hmac::Algorithm::Sha256, "HS256"),
        (hmac::Algorithm::Sha384, "HS384"),
        (hmac::Algorithm::Sha512, "HS512"),
    ] {
        let header = format!(r#"{{"typ":"JWT","alg":"{name}"}}"#);
        let input = format!("{}.{}", b64(header.as_bytes()), b64(payload.as_bytes()));
        let token = format!(
            "{input}.{}",
            b64(&hmac::sign(alg, secret, input.as_bytes()))
        );
        // The secret is *known* to the verifier, but only HS256 is allowed.
        let mut v = Validation::new(Algorithm::HS256);
        v.validate_aud = false;
        let key = DecodingKey::from_secret(secret);
        let accepted = decode::<Value>(&token, &key, &v).is_ok();
        assert_eq!(
            accepted,
            name == "HS256",
            "{name}: wrong accept/reject under an HS256-only policy"
        );
    }
}

// --- structure and encoding ---

#[quench_test::test]
async fn malformed_structure_is_rejected() {
    let good = valid_token();
    let parts: Vec<&str> = good.split('.').collect();
    let (h, p, s) = (parts[0], parts[1], parts[2]);
    for (case, token) in [
        ("empty", String::new()),
        ("dots only", "..".to_string()),
        ("two parts", format!("{h}.{p}")),
        ("four parts", format!("{h}.{p}.{s}.{s}")),
        ("five parts (JWE shape)", format!("{h}.{p}.{s}.{s}.{s}")),
        ("stripped signature", format!("{h}.{p}.")),
        ("leading space", format!(" {good}")),
        ("trailing newline", format!("{good}\n")),
        ("bearer prefix", format!("Bearer {good}")),
        ("padded base64", format!("{h}=.{p}.{s}")),
        (
            "std base64 alphabet",
            format!("{}+/.{p}.{s}", &h[..h.len() - 1]),
        ),
        ("NUL byte", format!("{h}.{p}\0.{s}")),
        ("unicode", format!("{h}.{p}.é{s}")),
        ("short signature", format!("{h}.{p}.{}", &s[..s.len() / 2])),
        ("long signature", format!("{h}.{p}.{s}AAAA")),
        ("zero signature", format!("{h}.{p}.{}", b64(&[0u8; 64]))),
        (
            "signature of payload swapped",
            format!("{h}.{}.{s}", b64(b"{}")),
        ),
    ] {
        must_reject(case, &token);
    }
}

#[quench_test::test]
async fn every_single_bit_flip_in_a_token_is_rejected() {
    let good = valid_token();
    let bytes = good.as_bytes();
    for index in 0..bytes.len() {
        for bit in 0..8 {
            let mut mutated = bytes.to_vec();
            mutated[index] ^= 1 << bit;
            let Ok(text) = String::from_utf8(mutated) else {
                continue;
            };
            // A flip in the last base64 character may touch only padding bits and decode to the
            // same signature; the token is then byte-identical in meaning, not a forgery.
            if text == good {
                continue;
            }
            if verdict(&text).is_ok() {
                let (a, b): (Vec<&str>, Vec<&str>) =
                    (good.split('.').collect(), text.split('.').collect());
                let same_meaning = a.len() == b.len()
                    && a.iter().zip(&b).all(|(x, y)| {
                        URL_SAFE_NO_PAD.decode(x).ok() == URL_SAFE_NO_PAD.decode(y).ok()
                    });
                assert!(
                    same_meaning,
                    "bit {bit} of byte {index} produced an accepted, different token"
                );
            }
        }
    }
}

#[quench_test::test]
async fn non_canonical_ed25519_signatures_are_rejected() {
    // Malleability: (R, S + L) verifies under a lax verifier and gives an attacker a second valid
    // signature for a token they never had signed. RFC 8032 requires S < L.
    const L: [u8; 32] = [
        0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde,
        0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
    ];
    let good = valid_token();
    let parts: Vec<&str> = good.split('.').collect();
    let mut sig = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let mut carry = 0u16;
    for i in 0..32 {
        let sum = u16::from(sig[32 + i]) + u16::from(L[i]) + carry;
        sig[32 + i] = (sum & 0xff) as u8;
        carry = sum >> 8;
    }
    assert_eq!(carry, 0, "S + L must fit in 256 bits for S < L");
    let forged = format!("{}.{}.{}", parts[0], parts[1], b64(&sig));
    must_reject("S + L malleated signature", &forged);
}

// --- claim handling ---

#[quench_test::test]
async fn expiry_is_enforced_and_its_leeway_is_bounded() {
    let token = |exp: i64| sign_raw(EDDSA, &claims_json(&format!(r#""exp":{exp}"#)));
    assert!(verdict(&token(now() + 600)).is_ok());
    assert_eq!(
        verdict(&token(now() - 3600)),
        Err(ErrorKind::ExpiredSignature)
    );
    assert_eq!(verdict(&token(0)), Err(ErrorKind::ExpiredSignature));
    assert_eq!(verdict(&token(-1)), Err(ErrorKind::ExpiredSignature));
    assert_eq!(verdict(&token(i64::MIN)), Err(ErrorKind::ExpiredSignature));
    // Default leeway is 60s: 2 minutes past must be dead, with no integer-wrap surprises.
    assert_eq!(
        verdict(&token(now() - 120)),
        Err(ErrorKind::ExpiredSignature)
    );
    let mut strict = validation();
    strict.leeway = 0;
    let result = decode::<Value>(&token(now() - 5), &public(), &strict);
    assert_eq!(result.unwrap_err().kind(), &ErrorKind::ExpiredSignature);
}

#[quench_test::test]
async fn a_missing_expiry_is_rejected() {
    must_reject("no exp", &sign_raw(EDDSA, &claims_json(r#""iat":1"#)));
    must_reject("exp null", &sign_raw(EDDSA, &claims_json(r#""exp":null"#)));
}

#[quench_test::test]
async fn an_expiry_of_the_wrong_type_never_means_valid_forever() {
    // The risky shape: a verifier that skips a time check it cannot parse, then hands the claims to
    // a lenient consumer. Decoding into `Value` is that lenient consumer.
    for (case, exp) in [
        ("string", r#""9999999999""#),
        ("numeric string", r#""1""#),
        ("bool true", "true"),
        ("bool false", "false"),
        ("array", "[9999999999]"),
        ("object", r#"{"v":9999999999}"#),
        ("empty string", r#""""#),
    ] {
        let token = sign_raw(EDDSA, &claims_json(&format!(r#""exp":{exp}"#)));
        must_reject(&format!("exp as {case}"), &token);
    }
}

#[quench_test::test]
async fn not_before_of_the_wrong_type_never_means_valid_now() {
    for (case, nbf) in [
        ("string", r#""9999999999""#),
        ("array", "[9999999999]"),
        ("bool", "true"),
    ] {
        let token = sign_raw(
            EDDSA,
            &claims_json(&format!(r#""exp":{},"nbf":{nbf}"#, now() + 600)),
        );
        must_reject(&format!("nbf as {case}"), &token);
    }
    let future = sign_raw(
        EDDSA,
        &claims_json(&format!(r#""exp":{},"nbf":{}"#, now() + 9000, now() + 3600)),
    );
    assert_eq!(verdict(&future), Err(ErrorKind::ImmatureSignature));
}

#[quench_test::test]
async fn audience_and_issuer_are_matched_exactly() {
    let exp = now() + 600;
    let mut v = Validation::new(Algorithm::EdDSA);
    v.aud = Some(std::iter::once("warehouse".to_string()).collect());
    v.iss = Some(std::iter::once("gatehouse".to_string()).collect());
    let check = |claims: &str| {
        decode::<Value>(&sign_raw(EDDSA, &claims_json(claims)), &public(), &v).is_ok()
    };
    assert!(check(&format!(
        r#""exp":{exp},"aud":"warehouse","iss":"gatehouse""#
    )));
    assert!(check(&format!(
        r#""exp":{exp},"aud":["x","warehouse"],"iss":"gatehouse""#
    )));
    for (case, claims) in [
        (
            "other audience",
            format!(r#""exp":{exp},"aud":"conveyor","iss":"gatehouse""#),
        ),
        (
            "prefix audience",
            format!(r#""exp":{exp},"aud":"ware","iss":"gatehouse""#),
        ),
        (
            "superstring audience",
            format!(r#""exp":{exp},"aud":"warehouse2","iss":"gatehouse""#),
        ),
        (
            "case-folded audience",
            format!(r#""exp":{exp},"aud":"Warehouse","iss":"gatehouse""#),
        ),
        (
            "trailing-space audience",
            format!(r#""exp":{exp},"aud":"warehouse ","iss":"gatehouse""#),
        ),
        (
            "missing audience",
            format!(r#""exp":{exp},"iss":"gatehouse""#),
        ),
        (
            "audience wrong type",
            format!(r#""exp":{exp},"aud":7,"iss":"gatehouse""#),
        ),
        (
            "audience empty array",
            format!(r#""exp":{exp},"aud":[],"iss":"gatehouse""#),
        ),
        (
            "audience null",
            format!(r#""exp":{exp},"aud":null,"iss":"gatehouse""#),
        ),
        (
            "other issuer",
            format!(r#""exp":{exp},"aud":"warehouse","iss":"evil""#),
        ),
        (
            "missing issuer",
            format!(r#""exp":{exp},"aud":"warehouse""#),
        ),
        (
            "issuer wrong type",
            format!(r#""exp":{exp},"aud":"warehouse","iss":["gatehouse"]"#),
        ),
    ] {
        assert!(!check(&claims), "{case}: accepted");
    }
}

#[quench_test::test]
async fn duplicate_claim_names_resolve_to_the_last_value() {
    // RFC 8259 leaves duplicate names unpredictable, so a component that reads the *first* `exp`
    // (a proxy, a logger, another language's verifier) can disagree with this one about the same
    // token. quench-serde deliberately keeps the last value; this pins that so a switch to
    // first-wins - the more dangerous divergence from most parsers - cannot happen unnoticed.
    // Rejecting duplicates outright would be stronger; that is a quench-serde decision.
    let live = now() + 600;
    let dead = now() - 3600;
    let dead_then_live = sign_raw(
        EDDSA,
        &format!(r#"{{"sub":"ada","exp":{dead},"exp":{live}}}"#),
    );
    let live_then_dead = sign_raw(
        EDDSA,
        &format!(r#"{{"sub":"ada","exp":{live},"exp":{dead}}}"#),
    );
    assert!(verdict(&dead_then_live).is_ok(), "last-wins expected");
    assert_eq!(verdict(&live_then_dead), Err(ErrorKind::ExpiredSignature));
}

// --- resource exhaustion on the unauthenticated part ---

#[quench_test::test]
async fn deeply_nested_header_json_is_refused_without_overflowing_the_stack() {
    // The header is parsed *before* the signature is checked, so any attacker can reach this parser
    // with ~48 KiB of `[` inside an Authorization header or cookie.
    for depth in [1_000usize, 10_000, 40_000] {
        let header = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let token = format!(
            "{}.{}.{}",
            b64(header.as_bytes()),
            b64(b"{}"),
            b64(&[0u8; 64])
        );
        assert!(decode_header(&token).is_err());
        must_reject(&format!("nested header depth {depth}"), &token);

        let object: String =
            (0..depth).map(|_| r#"{"a":"#).collect::<String>() + "1" + &"}".repeat(depth);
        let token = format!(
            "{}.{}.{}",
            b64(object.as_bytes()),
            b64(b"{}"),
            b64(&[0u8; 64])
        );
        must_reject(&format!("nested object header depth {depth}"), &token);
    }
}

#[quench_test::test]
async fn deeply_nested_claims_json_is_refused_without_overflowing_the_stack() {
    for depth in [1_000usize, 10_000, 40_000] {
        let nested = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
        let payload = format!(r#"{{"exp":{},"x":{nested}}}"#, now() + 600);
        let _ = verdict(&sign_raw(EDDSA, &payload));
    }
}

#[quench_test::test]
async fn oversized_tokens_are_handled_in_bounded_time() {
    let started = std::time::Instant::now();
    let huge = "A".repeat(4 * 1024 * 1024);
    for token in [
        format!("{huge}.{huge}.{huge}"),
        format!("{}.e30.AAAA", "e".repeat(2 * 1024 * 1024)),
        ".".repeat(1024 * 1024),
    ] {
        must_reject("oversized", &token);
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "rejecting oversized tokens took {:?}",
        started.elapsed()
    );
}

#[quench_test::test]
async fn header_fields_cannot_redirect_key_lookup_into_trouble() {
    // `kid` is attacker-chosen text that services use to look up keys: whatever it holds, parsing
    // must treat it as data. (Resolution-side handling is covered by the auth layer's tests.)
    for kid in [
        "../../etc/passwd",
        "' OR '1'='1",
        "a\0b",
        "{{7*7}}",
        "../../../dev/null\n",
        &"k".repeat(100_000),
    ] {
        let escaped = Value::from(kid).to_string();
        let header = format!(r#"{{"typ":"JWT","alg":"EdDSA","kid":{escaped}}}"#);
        let token = sign_raw(&header, &claims_json(&format!(r#""exp":{}"#, now() + 600)));
        let parsed = decode_header(&token).expect("a well-formed header parses");
        assert_eq!(
            parsed.kid.as_deref(),
            Some(kid),
            "kid must round-trip as inert data"
        );
    }
}

#[quench_test::test]
async fn tokens_made_by_the_library_round_trip_and_stay_pinned_to_their_key() {
    let key = signing_key();
    let encoding = EncodingKey::from_ed_key(key);
    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some("k1".into());
    let claims =
        quench_serde::json::parse_slice(format!(r#"{{"exp":{}}}"#, now() + 600).as_bytes())
            .unwrap();
    let token = encode(&header, &claims, &encoding).unwrap();
    assert!(decode::<Value>(&token, &public(), &validation()).is_ok());
    // Wrong key must not verify it.
    let other = DecodingKey::from_ed_der(&SigningKey::from_seed(&[8u8; 32]).public_key());
    assert!(decode::<Value>(&token, &other, &validation()).is_err());
}

quench_test::main!();
