//! SCRAM-SHA-256 (RFC 5802 with RFC 7677's parameters), the client side, as
//! PostgreSQL speaks it. Channel binding is not used (`n,,` gs2 header).

use crate::error::{Error, Result};
use quench_core::base64::STANDARD;
use quench_crypto::hash::sha256;
use quench_crypto::hmac::Algorithm;
use quench_crypto::{ct, hmac, pbkdf2};

/// The mechanism name to send in `SASLInitialResponse`.
pub const MECHANISM: &str = "SCRAM-SHA-256";

/// Largest iteration count accepted from a server; real ones are 4096.
const MAX_ITERATIONS: u32 = 5_000_000;

/// One authentication exchange.
#[derive(Debug)]
pub struct Scram {
    password: Vec<u8>,
    client_nonce: String,
    client_first_bare: String,
    /// Filled by [`Scram::client_final`]: what the server's signature must be.
    expected_server_signature: Option<Vec<u8>>,
}

impl Scram {
    /// A fresh exchange with a random 24-byte nonce.
    #[must_use]
    pub fn new(password: &str) -> Self {
        Self::with_nonce(
            password,
            &STANDARD.encode(quench_core::random::bytes::<18>()),
        )
    }

    /// An exchange with a chosen nonce (for tests against published vectors).
    #[must_use]
    pub fn with_nonce(password: &str, nonce: &str) -> Self {
        Self {
            password: password.as_bytes().to_vec(),
            client_nonce: nonce.to_string(),
            client_first_bare: format!("n=,r={nonce}"),
            expected_server_signature: None,
        }
    }

    /// The client-first message.
    #[must_use]
    pub fn client_first(&self) -> Vec<u8> {
        format!("n,,{}", self.client_first_bare).into_bytes()
    }

    /// Answers the server-first message with the client-final message (including the proof).
    ///
    /// # Errors
    ///
    /// [`Error::Auth`] when the server's message is malformed, does not extend our nonce,
    /// or asks for an unreasonable iteration count.
    pub fn client_final(&mut self, server_first: &[u8]) -> Result<Vec<u8>> {
        let server_first = std::str::from_utf8(server_first)
            .map_err(|_| Error::Auth("the server's SCRAM message is not UTF-8".into()))?;
        let (mut nonce, mut salt, mut iterations) = (None, None, None);
        for attribute in server_first.split(',') {
            match attribute.split_once('=') {
                Some(("r", value)) => nonce = Some(value),
                Some(("s", value)) => salt = Some(value),
                Some(("i", value)) => iterations = Some(value),
                Some(("m", _)) => {
                    return Err(Error::Auth(
                        "the server requires an unsupported SCRAM extension".into(),
                    ));
                }
                _ => {}
            }
        }
        let (Some(nonce), Some(salt), Some(iterations)) = (nonce, salt, iterations) else {
            return Err(Error::Auth("incomplete SCRAM server-first message".into()));
        };
        if !nonce.starts_with(&self.client_nonce) || nonce.len() == self.client_nonce.len() {
            return Err(Error::Auth(
                "the server's SCRAM nonce does not extend ours".into(),
            ));
        }
        let salt = STANDARD
            .decode(salt)
            .map_err(|_| Error::Auth("the SCRAM salt is not valid base64".into()))?;
        let iterations: u32 = iterations
            .parse()
            .map_err(|_| Error::Auth("the SCRAM iteration count is not a number".into()))?;
        if iterations == 0 || iterations > MAX_ITERATIONS {
            return Err(Error::Auth(format!(
                "unreasonable SCRAM iteration count {iterations}"
            )));
        }

        let mut salted = [0u8; 32];
        pbkdf2::derive(
            Algorithm::Sha256,
            iterations,
            &salt,
            &self.password,
            &mut salted,
        );
        let client_key = hmac::sign(Algorithm::Sha256, &salted, "Client Key");
        let stored_key = sha256(&client_key);
        let client_final_without_proof = format!("c=biws,r={nonce}");
        let auth_message = format!(
            "{},{server_first},{client_final_without_proof}",
            self.client_first_bare
        );
        let client_signature = hmac::sign(Algorithm::Sha256, &stored_key, &auth_message);
        let proof: Vec<u8> = client_key
            .iter()
            .zip(&client_signature)
            .map(|(a, b)| a ^ b)
            .collect();

        let server_key = hmac::sign(Algorithm::Sha256, &salted, "Server Key");
        self.expected_server_signature =
            Some(hmac::sign(Algorithm::Sha256, &server_key, &auth_message));

        Ok(format!("{client_final_without_proof},p={}", STANDARD.encode(proof)).into_bytes())
    }

    /// Checks the server-final message: proof that the server knew the password too.
    ///
    /// # Errors
    ///
    /// [`Error::Auth`] when the server reports an error, or its signature is wrong.
    pub fn verify_server(&self, server_final: &[u8]) -> Result<()> {
        let text = std::str::from_utf8(server_final)
            .map_err(|_| Error::Auth("the server's final SCRAM message is not UTF-8".into()))?;
        if let Some(reason) = text.strip_prefix("e=") {
            return Err(Error::Auth(format!(
                "the server rejected the SCRAM exchange: {reason}"
            )));
        }
        let signature = text.strip_prefix("v=").ok_or_else(|| {
            Error::Auth("the server's final SCRAM message has no signature".into())
        })?;
        let signature = STANDARD
            .decode(signature)
            .map_err(|_| Error::Auth("the SCRAM server signature is not valid base64".into()))?;
        let expected = self
            .expected_server_signature
            .as_ref()
            .ok_or_else(|| Error::Auth("SCRAM final message before the client-final".into()))?;
        if ct::eq(&signature, expected) {
            Ok(())
        } else {
            Err(Error::Auth(
                "the server's SCRAM signature does not match: it does not know the password".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 7677 section 3.
    const NONCE: &str = "rOprNGfwEbeRWgbNEkqO";
    const SERVER_FIRST: &str =
        "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";

    #[quench_test::test]
    fn rfc_7677_exchange() {
        let mut scram = Scram::with_nonce("pencil", NONCE);
        assert_eq!(scram.client_first(), b"n,,n=,r=rOprNGfwEbeRWgbNEkqO");
        let client_final = scram.client_final(SERVER_FIRST.as_bytes()).unwrap();
        // The RFC's client-first has `n=user`; PostgreSQL ignores the name, so the proof differs from
        // the RFC's. What must hold is that the server can verify it, which the live tests check; here
        // the structure and the verifiable server signature are checked instead.
        let text = String::from_utf8(client_final).unwrap();
        assert!(
            text.starts_with("c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p="),
            "{text}"
        );
        assert!(scram.verify_server(b"v=AAAA").is_err());
    }

    #[quench_test::test]
    fn rfc_7677_proof_and_server_signature_with_the_rfc_username() {
        // Reproduce the RFC's exact exchange by using its client-first-bare (`n=user,r=...`).
        let mut scram = Scram::with_nonce("pencil", NONCE);
        scram.client_first_bare = format!("n=user,r={NONCE}");
        let client_final =
            String::from_utf8(scram.client_final(SERVER_FIRST.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            client_final,
            "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
        );
        scram
            .verify_server(b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")
            .unwrap();
        let wrong = scram
            .verify_server(b"v=7rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")
            .unwrap_err();
        assert!(wrong.to_string().contains("does not know the password"));
    }

    #[quench_test::test]
    fn random_nonces_differ() {
        let a = Scram::new("pw").client_first();
        let b = Scram::new("pw").client_first();
        assert_ne!(a, b);
        assert!(a.starts_with(b"n,,n=,r="));
    }

    #[quench_test::test]
    fn malformed_server_messages_are_rejected() {
        let new = || Scram::with_nonce("pw", "abc");
        for bad in [
            "r=abcXYZ,s=c2FsdA==",
            "s=c2FsdA==,i=4096",
            "r=abcXYZ,i=4096",
            "r=wrongXYZ,s=c2FsdA==,i=4096",
            "r=abc,s=c2FsdA==,i=4096",
            "r=abcXYZ,s=!!!,i=4096",
            "r=abcXYZ,s=c2FsdA==,i=many",
            "r=abcXYZ,s=c2FsdA==,i=0",
            "r=abcXYZ,s=c2FsdA==,i=99999999",
            "m=ext,r=abcXYZ,s=c2FsdA==,i=4096",
        ] {
            assert!(
                matches!(new().client_final(bad.as_bytes()), Err(Error::Auth(_))),
                "{bad}"
            );
        }
        assert!(new().client_final(&[0xff, 0xfe]).is_err());
        assert!(
            new()
                .client_final(b"r=abcXYZ,s=c2FsdA==,i=1,x=ignored")
                .is_ok(),
            "unknown attributes are ignored"
        );
    }

    #[quench_test::test]
    fn server_final_errors() {
        let mut scram = Scram::with_nonce("pw", "abc");
        assert!(
            scram.verify_server(b"v=AAAA").is_err(),
            "no client-final yet"
        );
        scram.client_final(b"r=abcXYZ,s=c2FsdA==,i=1").unwrap();
        for bad in ["e=invalid-proof", "x=1", "v=not base64!"] {
            assert!(scram.verify_server(bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(scram.verify_server(&[0xff]).is_err());
        assert!(
            scram
                .verify_server(b"e=invalid-proof")
                .unwrap_err()
                .to_string()
                .contains("invalid-proof")
        );
    }
}
