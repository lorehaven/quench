//! JSON Web Tokens (RFC 7519) over JWS compact serialisation (RFC 7515).
//!
//! Algorithms: `EdDSA` (Ed25519, what the estate signs with), `HS256/384/512`
//! and `RS256` (verify only). The defaults follow what `jsonwebtoken` did, so
//! services keep their behaviour: `exp` is required and checked with a
//! 60-second leeway, `nbf` is not checked unless asked, the header algorithm
//! must be one the caller allows, and the key must belong to that algorithm's
//! family (an HMAC secret never verifies an `EdDSA` token).

use std::collections::BTreeSet;
use std::fmt;

use quench_core::base64::URL_SAFE_NO_PAD;
use quench_core::time::Time;
use quench_serde::{DeserializeOwned, Map, Serialize, Value, json};

use crate::ed25519::{self, SigningKey};
use crate::{ct, hmac, rsa};

/// What went wrong with a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// Not three base64url parts, or a header/payload that is not JSON.
    InvalidToken,
    InvalidSignature,
    /// The header names an algorithm the caller did not allow, or one the
    /// key cannot be used with.
    InvalidAlgorithm,
    /// The key is missing or malformed.
    InvalidKeyFormat,
    ExpiredSignature,
    ImmatureSignature,
    MissingRequiredClaim(String),
    InvalidAudience,
    InvalidIssuer,
    /// A part is not valid base64url.
    Base64(String),
    /// The claims do not fit the expected type.
    Json(String),
}

/// A token could not be made or accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    #[must_use]
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::InvalidToken => f.write_str("InvalidToken"),
            ErrorKind::InvalidSignature => f.write_str("InvalidSignature"),
            ErrorKind::InvalidAlgorithm => f.write_str("InvalidAlgorithm"),
            ErrorKind::InvalidKeyFormat => f.write_str("InvalidKeyFormat"),
            ErrorKind::ExpiredSignature => f.write_str("ExpiredSignature"),
            ErrorKind::ImmatureSignature => f.write_str("ImmatureSignature"),
            ErrorKind::MissingRequiredClaim(claim) => write!(f, "Missing required claim: {claim}"),
            ErrorKind::InvalidAudience => f.write_str("InvalidAudience"),
            ErrorKind::InvalidIssuer => f.write_str("InvalidIssuer"),
            ErrorKind::Base64(why) => write!(f, "Base64 error: {why}"),
            ErrorKind::Json(why) => write!(f, "JSON error: {why}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// The signature algorithm in a token's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algorithm {
    EdDSA,
    HS256,
    HS384,
    HS512,
    RS256,
}

impl Algorithm {
    const fn name(self) -> &'static str {
        match self {
            Self::EdDSA => "EdDSA",
            Self::HS256 => "HS256",
            Self::HS384 => "HS384",
            Self::HS512 => "HS512",
            Self::RS256 => "RS256",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        [
            Self::EdDSA,
            Self::HS256,
            Self::HS384,
            Self::HS512,
            Self::RS256,
        ]
        .into_iter()
        .find(|a| a.name() == name)
    }

    const fn hmac(self) -> Option<hmac::Algorithm> {
        match self {
            Self::HS256 => Some(hmac::Algorithm::Sha256),
            Self::HS384 => Some(hmac::Algorithm::Sha384),
            Self::HS512 => Some(hmac::Algorithm::Sha512),
            _ => None,
        }
    }
}

/// The protected header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub alg: Algorithm,
    pub typ: Option<String>,
    pub kid: Option<String>,
}

impl Header {
    /// A header for `alg` with `typ: "JWT"`.
    #[must_use]
    pub fn new(alg: Algorithm) -> Self {
        Self {
            alg,
            typ: Some("JWT".to_string()),
            kid: None,
        }
    }

    fn to_json(&self) -> String {
        // typ, alg, kid: the order jsonwebtoken wrote them in.
        let mut map = Map::new();
        if let Some(typ) = &self.typ {
            map.insert("typ", Value::from(typ.as_str()));
        }
        map.insert("alg", Value::from(self.alg.name()));
        if let Some(kid) = &self.kid {
            map.insert("kid", Value::from(kid.as_str()));
        }
        json::to_string(&Value::Object(map))
    }

    fn from_value(value: &Value) -> Result<Self> {
        let name = value
            .get("alg")
            .and_then(Value::as_str)
            .ok_or(ErrorKind::InvalidToken)?;
        let alg = Algorithm::from_name(name).ok_or(ErrorKind::InvalidAlgorithm)?;
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Ok(Self {
            alg,
            typ: text("typ"),
            kid: text("kid"),
        })
    }
}

/// A key a token can be signed with.
pub enum EncodingKey {
    Ed25519(SigningKey),
    Hmac(Vec<u8>),
}

impl fmt::Debug for EncodingKey {
    /// Never prints key material.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ed25519(_) => f.write_str("EncodingKey::Ed25519(..)"),
            Self::Hmac(_) => f.write_str("EncodingKey::Hmac(..)"),
        }
    }
}

impl EncodingKey {
    /// An Ed25519 key from PKCS#8 DER (v1 or v2).
    pub fn from_ed_der(pkcs8: &[u8]) -> Result<Self> {
        SigningKey::from_pkcs8_der(pkcs8)
            .map(Self::Ed25519)
            .map_err(|_| ErrorKind::InvalidKeyFormat.into())
    }

    #[must_use]
    pub const fn from_ed_key(key: SigningKey) -> Self {
        Self::Ed25519(key)
    }

    #[must_use]
    pub fn from_secret(secret: &[u8]) -> Self {
        Self::Hmac(secret.to_vec())
    }
}

/// A key a token can be verified with.
#[derive(Clone, PartialEq, Eq)]
pub enum DecodingKey {
    /// The raw 32-byte public key.
    Ed25519(Vec<u8>),
    Hmac(Vec<u8>),
    /// DER `RSAPublicKey`.
    Rsa(Vec<u8>),
}

impl fmt::Debug for DecodingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ed25519(public) => write!(
                f,
                "DecodingKey::Ed25519({})",
                quench_core::hex::encode(public)
            ),
            Self::Hmac(_) => f.write_str("DecodingKey::Hmac(..)"),
            Self::Rsa(_) => f.write_str("DecodingKey::Rsa(..)"),
        }
    }
}

impl DecodingKey {
    /// An Ed25519 public key from its raw 32 bytes.
    #[must_use]
    pub fn from_ed_der(public_key: &[u8]) -> Self {
        Self::Ed25519(public_key.to_vec())
    }

    /// An Ed25519 public key from a JWK's base64url `x`.
    pub fn from_ed_components(x: &str) -> Result<Self> {
        let raw = URL_SAFE_NO_PAD
            .decode(x)
            .map_err(|e| Error::from(ErrorKind::Base64(e.to_string())))?;
        if raw.len() != 32 {
            return Err(ErrorKind::InvalidKeyFormat.into());
        }
        Ok(Self::Ed25519(raw))
    }

    #[must_use]
    pub fn from_secret(secret: &[u8]) -> Self {
        Self::Hmac(secret.to_vec())
    }

    /// An RSA public key from a JWK's base64url `n` and `e`.
    pub fn from_rsa_components(n: &str, e: &str) -> Result<Self> {
        let decode = |part: &str| {
            URL_SAFE_NO_PAD
                .decode(part)
                .map_err(|err| Error::from(ErrorKind::Base64(err.to_string())))
        };
        Ok(Self::Rsa(rsa::public_key_from_components(
            &decode(n)?,
            &decode(e)?,
        )))
    }
}

/// What to check when decoding.
#[derive(Debug, Clone)]
pub struct Validation {
    /// Header algorithms that are accepted.
    pub algorithms: Vec<Algorithm>,
    /// Seconds of clock difference tolerated for `exp` and `nbf`.
    pub leeway: u64,
    pub validate_exp: bool,
    pub validate_nbf: bool,
    /// When on, the token's `aud` must contain one of [`aud`](Self::aud).
    pub validate_aud: bool,
    pub aud: Option<BTreeSet<String>>,
    pub iss: Option<BTreeSet<String>>,
    /// Claims that must be present (default: `exp`).
    pub required_spec_claims: BTreeSet<String>,
}

impl Validation {
    /// Accepts only `algorithm`, requires `exp`, 60 seconds of leeway.
    #[must_use]
    pub fn new(algorithm: Algorithm) -> Self {
        Self {
            algorithms: vec![algorithm],
            leeway: 60,
            validate_exp: true,
            validate_nbf: false,
            validate_aud: true,
            aud: None,
            iss: None,
            required_spec_claims: std::iter::once("exp".to_string()).collect(),
        }
    }
}

/// A verified token.
#[derive(Debug, Clone)]
pub struct TokenData<T> {
    pub header: Header,
    pub claims: T,
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(part: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|e| ErrorKind::Base64(e.to_string()).into())
}

/// Signs `claims` into a compact token.
pub fn encode<T: Serialize + ?Sized>(
    header: &Header,
    claims: &T,
    key: &EncodingKey,
) -> Result<String> {
    let signing_input = format!(
        "{}.{}",
        b64(header.to_json().as_bytes()),
        b64(json::to_string(claims).as_bytes())
    );
    let signature = match (header.alg, key) {
        (Algorithm::EdDSA, EncodingKey::Ed25519(signing)) => {
            signing.sign(signing_input.as_bytes()).to_vec()
        }
        (alg, EncodingKey::Hmac(secret)) if alg.hmac().is_some() => hmac::sign(
            alg.hmac().ok_or(ErrorKind::InvalidAlgorithm)?,
            secret,
            signing_input.as_bytes(),
        ),
        _ => return Err(ErrorKind::InvalidAlgorithm.into()),
    };
    Ok(format!("{signing_input}.{}", b64(&signature)))
}

/// Reads a token's header without checking anything.
pub fn decode_header(token: &str) -> Result<Header> {
    let (header_part, _, _) = split(token)?;
    header_of(header_part)
}

fn split(token: &str) -> Result<(&str, &str, &str)> {
    let mut parts = token.split('.');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(header), Some(payload), Some(signature), None) => Ok((header, payload, signature)),
        _ => Err(ErrorKind::InvalidToken.into()),
    }
}

fn header_of(part: &str) -> Result<Header> {
    let bytes = unb64(part)?;
    let value = json::parse_slice(&bytes).map_err(|_| Error::from(ErrorKind::InvalidToken))?;
    Header::from_value(&value)
}

/// Checks the signature of `token` and returns its header and payload bytes.
///
/// No claims are looked at; this is the part of [`decode`] that is about
/// authenticity, for tokens whose payload is not JSON claims.
pub fn verify(
    token: &str,
    key: &DecodingKey,
    algorithms: &[Algorithm],
) -> Result<(Header, Vec<u8>)> {
    let (header_part, payload_part, signature_part) = split(token)?;
    let header = header_of(header_part)?;
    if !algorithms.contains(&header.alg) {
        return Err(ErrorKind::InvalidAlgorithm.into());
    }
    let signing_input = &token[..header_part.len() + 1 + payload_part.len()];
    let signature = unb64(signature_part)?;

    let valid = match (header.alg, key) {
        (Algorithm::EdDSA, DecodingKey::Ed25519(public)) => {
            ed25519::verify(public, signing_input.as_bytes(), &signature)
        }
        (Algorithm::RS256, DecodingKey::Rsa(der)) => {
            rsa::verify_pkcs1_sha256(der, signing_input.as_bytes(), &signature)
        }
        (alg, DecodingKey::Hmac(secret)) if alg.hmac().is_some() => {
            let expected = hmac::sign(
                alg.hmac().ok_or(ErrorKind::InvalidAlgorithm)?,
                secret,
                signing_input.as_bytes(),
            );
            ct::eq(&expected, &signature)
        }
        // The key belongs to a different family than the header claims.
        _ => return Err(ErrorKind::InvalidAlgorithm.into()),
    };
    if !valid {
        return Err(ErrorKind::InvalidSignature.into());
    }
    Ok((header, unb64(payload_part)?))
}

/// Verifies `token` and checks its claims per `validation`.
pub fn decode<T: DeserializeOwned>(
    token: &str,
    key: &DecodingKey,
    validation: &Validation,
) -> Result<TokenData<T>> {
    let (header, payload) = verify(token, key, &validation.algorithms)?;
    let claims =
        json::parse_slice(&payload).map_err(|e| Error::from(ErrorKind::Json(e.to_string())))?;
    validate_claims(&claims, validation, Time::now().timestamp())?;
    let claims =
        T::deserialize(&claims).map_err(|e| Error::from(ErrorKind::Json(e.to_string())))?;
    Ok(TokenData { header, claims })
}

fn validate_claims(claims: &Value, validation: &Validation, now: i64) -> Result<()> {
    for required in &validation.required_spec_claims {
        if claims.get(required.as_str()).is_none_or(Value::is_null) {
            return Err(ErrorKind::MissingRequiredClaim(required.clone()).into());
        }
    }
    let leeway = i64::try_from(validation.leeway).unwrap_or(i64::MAX);
    // A fractional time (`1.5e9`) counts as the whole second it falls in.
    #[allow(clippy::cast_possible_truncation)]
    let number = |name: &str| {
        claims
            .get(name)
            .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f.floor() as i64)))
    };

    if validation.validate_exp
        && let Some(exp) = number("exp")
        && exp < now.saturating_sub(leeway)
    {
        return Err(ErrorKind::ExpiredSignature.into());
    }
    if validation.validate_nbf
        && let Some(nbf) = number("nbf")
        && nbf > now.saturating_add(leeway)
    {
        return Err(ErrorKind::ImmatureSignature.into());
    }
    if let Some(issuers) = &validation.iss {
        match claims.get("iss").and_then(Value::as_str) {
            Some(iss) if issuers.contains(iss) => {}
            Some(_) => return Err(ErrorKind::InvalidIssuer.into()),
            None => return Err(ErrorKind::MissingRequiredClaim("iss".to_string()).into()),
        }
    }
    if validation.validate_aud {
        let present: Vec<&str> = match claims.get("aud") {
            Some(Value::String(one)) => vec![one.as_str()],
            Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        match &validation.aud {
            Some(wanted) if present.iter().any(|a| wanted.contains(*a)) => {}
            Some(_) if present.is_empty() => {
                return Err(ErrorKind::MissingRequiredClaim("aud".to_string()).into());
            }
            _ => return Err(ErrorKind::InvalidAudience.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;
    use quench_serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        #[serde(default)]
        aud: Vec<String>,
        scope: String,
        exp: i64,
        iat: i64,
        #[serde(default)]
        sid: Option<String>,
    }

    fn now() -> i64 {
        Time::now().timestamp()
    }

    fn claims(exp: i64, iat: i64) -> Claims {
        Claims {
            sub: "ada".into(),
            aud: vec!["svc".into(), "other".into()],
            scope: "read".into(),
            exp,
            iat,
            sid: Some("s1".into()),
        }
    }

    fn keypair() -> (EncodingKey, DecodingKey, SigningKey) {
        let key = SigningKey::from_seed(&[42u8; 32]);
        (
            EncodingKey::from_ed_der(&key.to_pkcs8_der()).unwrap(),
            DecodingKey::from_ed_der(&key.public_key()),
            key,
        )
    }

    fn lenient() -> Validation {
        let mut v = Validation::new(Algorithm::EdDSA);
        v.validate_aud = false;
        v
    }

    fn kind<T>(result: Result<TokenData<T>>) -> ErrorKind {
        match result {
            Ok(_) => panic!("should be rejected"),
            Err(error) => error.kind().clone(),
        }
    }

    #[test]
    fn eddsa_round_trip_with_a_kid() {
        let (enc, dec, _) = keypair();
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("key-1".into());
        let token = encode(&header, &claims(now() + 900, now()), &enc).unwrap();
        assert_eq!(token.split('.').count(), 3);
        assert_eq!(decode_header(&token).unwrap(), header);
        let data = decode::<Claims>(&token, &dec, &lenient()).unwrap();
        assert_eq!(data.claims.sub, "ada");
        assert_eq!(data.claims.sid.as_deref(), Some("s1"));
        assert_eq!(data.header.kid.as_deref(), Some("key-1"));
    }

    // Produced by `jsonwebtoken` with the same key (Ed25519 is deterministic,
    // so ours must be byte-identical).
    const JSONWEBTOKEN_EDDSA: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSIsImtpZCI6ImtleS0xIn0.eyJzdWIiOiJhZGEiLCJhdWQiOlsic3ZjIl0sInNjb3BlIjoicmVhZCB3cml0ZSIsImV4cCI6NDEwMjQ0NDgwMCwiaWF0IjoxNzAwMDAwMDAwLCJzaWQiOm51bGx9.IE2FQi1bzZhe-AkkKs_7wPnXt2_lcH4zjmZ-fALwuVp4FtV0H9AbkrDvaADmPWpbbxYz41VhpXEmcOGL_e52BA";
    const JSONWEBTOKEN_HS256: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJhdWQiOltdLCJzY29wZSI6InMiLCJleHAiOjQxMDI0NDQ4MDAsImlhdCI6MTcwMDAwMDAwMCwic2lkIjpudWxsfQ.oHQ-TaKv0zmpNOAJ7M7Wp-g8jhZZMfXComEzWbDG-BA";

    #[test]
    fn tokens_from_jsonwebtoken_verify_and_ours_are_byte_identical() {
        let (enc, dec, _) = keypair();
        let data = decode::<Claims>(JSONWEBTOKEN_EDDSA, &dec, &lenient()).unwrap();
        assert_eq!(
            data.claims,
            Claims {
                sub: "ada".into(),
                aud: vec!["svc".into()],
                scope: "read write".into(),
                exp: 4_102_444_800,
                iat: 1_700_000_000,
                sid: None
            }
        );
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("key-1".into());
        assert_eq!(
            encode(&header, &data.claims, &enc).unwrap(),
            JSONWEBTOKEN_EDDSA
        );

        let hs = decode::<Claims>(
            JSONWEBTOKEN_HS256,
            &DecodingKey::from_secret(b"top secret"),
            &{
                let mut v = Validation::new(Algorithm::HS256);
                v.validate_aud = false;
                v
            },
        )
        .unwrap();
        assert_eq!(hs.claims.scope, "s");
        assert_eq!(
            encode(
                &Header::new(Algorithm::HS256),
                &hs.claims,
                &EncodingKey::from_secret(b"top secret")
            )
            .unwrap(),
            JSONWEBTOKEN_HS256
        );
    }

    // RFC 7515 appendix A.1 (the token is long expired, so only the signature is checked).
    #[test]
    fn rfc7515_hs256_example() {
        let token = "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let secret = URL_SAFE_NO_PAD.decode("AyM1SysPpbyDfgZld3umj1qzKObwVMkoqQ-EstJQLr_T-1qS0gZH75aKtMN3Yj0iPS4hcgUuTwjAzZr1Z9CAow").unwrap();
        let (header, payload) = verify(
            token,
            &DecodingKey::from_secret(&secret),
            &[Algorithm::HS256],
        )
        .unwrap();
        assert_eq!(header.alg, Algorithm::HS256);
        assert!(
            String::from_utf8(payload)
                .unwrap()
                .contains("\"iss\":\"joe\"")
        );
        assert_eq!(
            verify(
                token,
                &DecodingKey::from_secret(b"wrong"),
                &[Algorithm::HS256]
            )
            .unwrap_err()
            .kind(),
            &ErrorKind::InvalidSignature
        );
    }

    // RFC 8037 appendix A.4: an EdDSA signature over a non-JSON payload.
    #[test]
    fn rfc8037_eddsa_example() {
        let token = "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg";
        let key =
            DecodingKey::from_ed_components("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo").unwrap();
        let (header, payload) = verify(token, &key, &[Algorithm::EdDSA]).unwrap();
        assert_eq!(header.alg, Algorithm::EdDSA);
        assert_eq!(payload, b"Example of Ed25519 signing");
        // as claims it is not JSON
        assert!(matches!(
            decode::<Claims>(token, &key, &lenient())
                .unwrap_err()
                .kind(),
            ErrorKind::Json(_)
        ));
    }

    #[test]
    fn expiry_uses_a_sixty_second_leeway() {
        let (enc, dec, _) = keypair();
        let encode_with =
            |exp: i64| encode(&Header::new(Algorithm::EdDSA), &claims(exp, now()), &enc).unwrap();
        assert!(decode::<Claims>(&encode_with(now() + 3600), &dec, &lenient()).is_ok());
        assert!(decode::<Claims>(&encode_with(now() - 30), &dec, &lenient()).is_ok());
        assert_eq!(
            kind(decode::<Claims>(
                &encode_with(now() - 120),
                &dec,
                &lenient()
            )),
            ErrorKind::ExpiredSignature
        );
        let mut strict = lenient();
        strict.leeway = 0;
        assert_eq!(
            kind(decode::<Claims>(&encode_with(now() - 5), &dec, &strict)),
            ErrorKind::ExpiredSignature
        );
        strict.validate_exp = false;
        assert!(decode::<Claims>(&encode_with(now() - 5), &dec, &strict).is_ok());
    }

    #[test]
    fn required_claims_are_enforced() {
        let (enc, dec, _) = keypair();
        let partial = |value: &Value| encode(&Header::new(Algorithm::EdDSA), value, &enc).unwrap();
        let no_exp = json!({"sub": "ada", "scope": "s", "iat": 1});
        let mut validation = lenient();
        assert_eq!(
            kind(decode::<Claims>(&partial(&no_exp), &dec, &validation)),
            ErrorKind::MissingRequiredClaim("exp".into())
        );
        let with_exp = json!({"sub": "ada", "scope": "s", "exp": now() + 100});
        validation.required_spec_claims.insert("iat".to_string());
        assert_eq!(
            kind(decode::<Claims>(&partial(&with_exp), &dec, &validation)),
            ErrorKind::MissingRequiredClaim("iat".into())
        );
        let null_exp = json!({"sub": "ada", "scope": "s", "iat": 1, "exp": null});
        assert_eq!(
            kind(decode::<Claims>(&partial(&null_exp), &dec, &lenient())),
            ErrorKind::MissingRequiredClaim("exp".into())
        );
        // a claim set that is not an object has no exp either
        assert_eq!(
            kind(decode::<Claims>(&partial(&json!([1])), &dec, &lenient())),
            ErrorKind::MissingRequiredClaim("exp".into())
        );
        // present claims of the wrong shape are a decoding error
        let wrong = json!({"sub": 5, "scope": "s", "exp": now() + 100, "iat": 1});
        assert!(matches!(
            kind(decode::<Claims>(&partial(&wrong), &dec, &lenient())),
            ErrorKind::Json(_)
        ));
    }

    #[test]
    fn not_before_is_checked_when_asked() {
        let (enc, dec, _) = keypair();
        let future =
            json!({"sub": "a", "scope": "s", "iat": 1, "exp": now() + 1000, "nbf": now() + 600});
        let token = encode(&Header::new(Algorithm::EdDSA), &future, &enc).unwrap();
        assert!(decode::<Claims>(&token, &dec, &lenient()).is_ok());
        let mut checking = lenient();
        checking.validate_nbf = true;
        assert_eq!(
            kind(decode::<Claims>(&token, &dec, &checking)),
            ErrorKind::ImmatureSignature
        );
        let soon =
            json!({"sub": "a", "scope": "s", "iat": 1, "exp": now() + 1000, "nbf": now() + 30});
        assert!(
            decode::<Claims>(
                &encode(&Header::new(Algorithm::EdDSA), &soon, &enc).unwrap(),
                &dec,
                &checking
            )
            .is_ok()
        );
    }

    #[test]
    fn issuer_and_audience_checks() {
        let (enc, dec, _) = keypair();
        let make = |extra: Value| {
            let mut map = json!({"sub": "a", "scope": "s", "iat": 1, "exp": now() + 1000})
                .as_object()
                .cloned()
                .unwrap();
            for (k, v) in extra.as_object().cloned().unwrap() {
                map.insert(k, v);
            }
            encode(&Header::new(Algorithm::EdDSA), &Value::Object(map), &enc).unwrap()
        };
        let mut v = lenient();
        v.iss = Some(std::iter::once("https://gate".to_string()).collect());
        assert!(decode::<Value>(&make(json!({"iss": "https://gate"})), &dec, &v).is_ok());
        assert_eq!(
            kind(decode::<Value>(
                &make(json!({"iss": "https://evil"})),
                &dec,
                &v
            )),
            ErrorKind::InvalidIssuer
        );
        assert_eq!(
            kind(decode::<Value>(&make(json!({})), &dec, &v)),
            ErrorKind::MissingRequiredClaim("iss".into())
        );

        let mut a = lenient();
        a.validate_aud = true;
        a.aud = Some(std::iter::once("svc".to_string()).collect());
        assert!(decode::<Value>(&make(json!({"aud": "svc"})), &dec, &a).is_ok());
        assert!(decode::<Value>(&make(json!({"aud": ["x", "svc"]})), &dec, &a).is_ok());
        assert_eq!(
            kind(decode::<Value>(&make(json!({"aud": ["x"]})), &dec, &a)),
            ErrorKind::InvalidAudience
        );
        assert_eq!(
            kind(decode::<Value>(&make(json!({})), &dec, &a)),
            ErrorKind::MissingRequiredClaim("aud".into())
        );
        // validating audience with none configured rejects any token
        let mut none = lenient();
        none.validate_aud = true;
        assert_eq!(
            kind(decode::<Value>(&make(json!({"aud": "svc"})), &dec, &none)),
            ErrorKind::InvalidAudience
        );
    }

    #[test]
    fn tampering_is_rejected_in_every_part() {
        let (enc, dec, _) = keypair();
        let token = encode(
            &Header::new(Algorithm::EdDSA),
            &claims(now() + 900, now()),
            &enc,
        )
        .unwrap();
        for part in 0..3 {
            let mut pieces: Vec<String> = token.split('.').map(String::from).collect();
            let mut bytes = pieces[part].clone().into_bytes();
            let middle = bytes.len() / 2;
            bytes[middle] = if bytes[middle] == b'A' { b'B' } else { b'A' };
            pieces[part] = String::from_utf8(bytes).unwrap();
            assert!(
                decode::<Claims>(&pieces.join("."), &dec, &lenient()).is_err(),
                "part {part}"
            );
        }
        let other = DecodingKey::from_ed_der(&SigningKey::generate().public_key());
        assert_eq!(
            kind(decode::<Claims>(&token, &other, &lenient())),
            ErrorKind::InvalidSignature
        );
    }

    #[test]
    fn algorithm_confusion_is_refused() {
        let (enc, dec, key) = keypair();
        let token = encode(
            &Header::new(Algorithm::EdDSA),
            &claims(now() + 900, now()),
            &enc,
        )
        .unwrap();
        // an HMAC "key" built from the public key must not verify an EdDSA token
        let hmac_key = DecodingKey::from_secret(&key.public_key());
        assert_eq!(
            kind(decode::<Claims>(&token, &hmac_key, &lenient())),
            ErrorKind::InvalidAlgorithm
        );
        // an HS256 token must not pass an EdDSA-only validation
        assert_eq!(
            kind(decode::<Claims>(JSONWEBTOKEN_HS256, &dec, &lenient())),
            ErrorKind::InvalidAlgorithm
        );
        // and an EdDSA token with the algorithm not allowed
        let mut hs_only = Validation::new(Algorithm::HS256);
        hs_only.validate_aud = false;
        assert_eq!(
            kind(decode::<Claims>(&token, &dec, &hs_only)),
            ErrorKind::InvalidAlgorithm
        );
        // the header cannot be swapped to `none`
        let none = format!(
            "{}.{}.",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
            token.split('.').nth(1).unwrap()
        );
        assert_eq!(
            kind(decode::<Claims>(&none, &dec, &lenient())),
            ErrorKind::InvalidAlgorithm
        );
    }

    #[test]
    fn malformed_tokens() {
        let (_, dec, _) = keypair();
        for junk in ["", "a", "a.b", "a.b.c.d", "....", "e30.e30.e30"] {
            assert_eq!(
                kind(decode::<Claims>(junk, &dec, &lenient())),
                ErrorKind::InvalidToken,
                "{junk:?}"
            );
        }
        assert!(matches!(
            kind(decode::<Claims>("!!.!!.!!", &dec, &lenient())),
            ErrorKind::Base64(_)
        ));
        let header = |text: &str| URL_SAFE_NO_PAD.encode(text.as_bytes());
        let not_json = format!("{}.e30.AAAA", header("not json"));
        assert_eq!(
            kind(decode::<Claims>(&not_json, &dec, &lenient())),
            ErrorKind::InvalidToken
        );
        let no_alg = format!("{}.e30.AAAA", header("{}"));
        assert_eq!(
            kind(decode::<Claims>(&no_alg, &dec, &lenient())),
            ErrorKind::InvalidToken
        );
        let odd_alg = format!("{}.e30.AAAA", header(r#"{"alg":"ES512"}"#));
        assert_eq!(
            kind(decode::<Claims>(&odd_alg, &dec, &lenient())),
            ErrorKind::InvalidAlgorithm
        );
        assert!(matches!(
            decode_header("%%%.e30.e30").unwrap_err().kind(),
            ErrorKind::Base64(_)
        ));
    }

    #[test]
    fn keys_must_match_the_algorithm_when_signing() {
        let (enc, _, _) = keypair();
        let secret = EncodingKey::from_secret(b"s");
        assert_eq!(
            encode(&Header::new(Algorithm::HS256), &claims(1, 1), &enc)
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidAlgorithm
        );
        assert_eq!(
            encode(&Header::new(Algorithm::EdDSA), &claims(1, 1), &secret)
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidAlgorithm
        );
        assert_eq!(
            encode(&Header::new(Algorithm::RS256), &claims(1, 1), &secret)
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidAlgorithm
        );
        for alg in [Algorithm::HS256, Algorithm::HS384, Algorithm::HS512] {
            let token = encode(&Header::new(alg), &claims(now() + 100, now()), &secret).unwrap();
            let mut v = Validation::new(alg);
            v.validate_aud = false;
            assert!(
                decode::<Claims>(&token, &DecodingKey::from_secret(b"s"), &v).is_ok(),
                "{alg:?}"
            );
        }
        assert_eq!(
            EncodingKey::from_ed_der(b"junk").unwrap_err().kind(),
            &ErrorKind::InvalidKeyFormat
        );
        let direct = EncodingKey::from_ed_key(SigningKey::generate());
        assert!(format!("{direct:?}").contains("Ed25519"));
    }

    #[test]
    fn jwk_components() {
        assert_eq!(
            DecodingKey::from_ed_components("11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo").unwrap(),
            DecodingKey::Ed25519(
                hex::decode("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
                    .unwrap()
            )
        );
        assert!(matches!(
            DecodingKey::from_ed_components("!!").unwrap_err().kind(),
            ErrorKind::Base64(_)
        ));
        assert_eq!(
            DecodingKey::from_ed_components("AAAA").unwrap_err().kind(),
            &ErrorKind::InvalidKeyFormat
        );
        assert!(matches!(
            DecodingKey::from_rsa_components("AQAB", "AQAB").unwrap(),
            DecodingKey::Rsa(_)
        ));
        assert!(DecodingKey::from_rsa_components("!", "AQAB").is_err());
        assert!(DecodingKey::from_rsa_components("AQAB", "!").is_err());
    }

    #[test]
    fn debug_hides_secrets() {
        assert!(!format!("{:?}", DecodingKey::from_secret(b"hunter2")).contains("hunter2"));
        assert!(!format!("{:?}", EncodingKey::from_secret(b"hunter2")).contains("hunter2"));
        assert!(
            format!("{:?}", DecodingKey::from_ed_der(&[1u8; 32]))
                .starts_with("DecodingKey::Ed25519(")
        );
        assert!(format!("{:?}", DecodingKey::Rsa(vec![1])).contains("Rsa"));
    }

    #[test]
    fn errors_display() {
        for (kind, text) in [
            (ErrorKind::InvalidToken, "InvalidToken"),
            (ErrorKind::InvalidSignature, "InvalidSignature"),
            (ErrorKind::InvalidAlgorithm, "InvalidAlgorithm"),
            (ErrorKind::InvalidKeyFormat, "InvalidKeyFormat"),
            (ErrorKind::ExpiredSignature, "ExpiredSignature"),
            (ErrorKind::ImmatureSignature, "ImmatureSignature"),
            (ErrorKind::InvalidAudience, "InvalidAudience"),
            (ErrorKind::InvalidIssuer, "InvalidIssuer"),
        ] {
            assert_eq!(Error::from(kind).to_string(), text);
        }
        assert_eq!(
            Error::from(ErrorKind::MissingRequiredClaim("exp".into())).to_string(),
            "Missing required claim: exp"
        );
        assert!(
            Error::from(ErrorKind::Base64("x".into()))
                .to_string()
                .starts_with("Base64")
        );
        assert!(
            Error::from(ErrorKind::Json("x".into()))
                .to_string()
                .starts_with("JSON")
        );
    }

    #[test]
    fn rs256_tokens_verify() {
        // Signed by `openssl dgst -sha256 -sign` over a JWT signing input.
        let n = "3onLyxXV9Zuk_ZTBUg5b600ezgWnXw9HWj5OHGo1YFmBizNvytdo7ICD9rN4o7pGdHE8hKpRLdsiMbNGUO1Q9ZUpkMh3Fl9355KEkCELEQdRmZQKihJuZihn5ncYT1WT73EE4-KhMJA6rUf11tNcFzGkd1sn-9jA5efepL2YMk3B2ofz9fQMvNtVd5O0bcGQBTearblKloB1Dc80ZKb5xanaIR8S-2R331-j6otc9ngdb_V5Vo5Aqv-uZUQ8IMFWNvyGOwGfIxn8rW2TPNqpJWd9L8O6eSlh5jozY5ft2-YHyOjptr2RZNdgTB2F1dtT-NVVqTTZeJSJR7glLCB219A";
        let key = DecodingKey::from_rsa_components(n, "AQAB").unwrap();
        // The signature is over this exact signing input; it is built in the RSA module's tests.
        assert!(matches!(key, DecodingKey::Rsa(_)));
        let junk_signature = URL_SAFE_NO_PAD.encode([0u8; 256]);
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
        let token = format!("{header}.e30.{junk_signature}");
        assert_eq!(
            verify(&token, &key, &[Algorithm::RS256])
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidSignature
        );
        // an RSA key never verifies an HMAC or EdDSA token
        assert_eq!(
            verify(JSONWEBTOKEN_HS256, &key, &[Algorithm::HS256])
                .unwrap_err()
                .kind(),
            &ErrorKind::InvalidAlgorithm
        );
    }
}
