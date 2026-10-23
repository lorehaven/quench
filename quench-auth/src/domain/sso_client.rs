//! The framework-agnostic half of the relying-party side of the
//! authorization-code + PKCE flow: config, the token-refresh HTTP call, and
//! the PKCE state shape.
//!
//! The parts that build a redirect response or read
//! the incoming request (`authorize_redirect`, `callback`, ...) live
//! in `crate::http::domain::sso_client`, since they're the only parts that
//! touch the request / response types.

use crate::domain::realm;
use quench_core::base64::URL_SAFE_NO_PAD;
use quench_serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct SsoConfig {
    pub client_id: String,
    #[cfg_attr(not(feature = "http"), allow(dead_code))]
    // only the http layer drives the sign-in flow
    pub(crate) client_secret: String,
    pub(crate) tls_verify: bool,
}

// Hand-written so the client secret can never end up in a log line.
impl std::fmt::Debug for SsoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsoConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .field("tls_verify", &self.tls_verify)
            .finish()
    }
}

impl SsoConfig {
    #[must_use]
    pub fn init() -> Self {
        let client_id = quench_core::env::get_or("GATEHOUSE_CLIENT_ID", "");
        let client_secret = quench_core::env::get_or("GATEHOUSE_CLIENT_SECRET", "");
        let tls_verify = quench_core::env::get_or("GATEHOUSE_TLS_VERIFY", "true")
            .parse()
            .unwrap_or(true);
        Self {
            client_id,
            client_secret,
            tls_verify,
        }
    }

    #[cfg_attr(not(feature = "http"), allow(dead_code))] // only the http layer drives the sign-in flow
    pub(crate) fn configured(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && realm::gatehouse_url().is_some()
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(not(feature = "http"), allow(dead_code))] // only the http layer drives the sign-in flow
pub(crate) struct AuthorizeState {
    pub(crate) state: String,
    pub(crate) code_verifier: String,
    pub(crate) redirect: String,
}

#[derive(Deserialize)]
pub(crate) struct TokenResponse {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
}

/// A fresh access/refresh pair, public where `TokenResponse` isn't - callers
/// outside this module have no business seeing gatehouse's wire shape.
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
}

// Hand-written so the tokens can never end up in a log line.
impl std::fmt::Debug for RefreshedTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshedTokens").finish_non_exhaustive()
    }
}

/// Trades a `forge_refresh` value for a fresh pair via gatehouse's `/refresh`;
/// `None` on any failure, so the caller just falls back to a full sign-in.
pub async fn refresh(refresh_token: &str) -> Option<RefreshedTokens> {
    let base = realm::gatehouse_url()?;
    let tls_verify: bool = quench_core::env::get_or("GATEHOUSE_TLS_VERIFY", "true")
        .parse()
        .unwrap_or(true);
    let client = quench_client::transport::Client::builder()
        .danger_accept_invalid_certs(!tls_verify)
        .build()
        .ok()?;
    let response = client
        .post(format!("{base}/api/v1/auth/refresh"))
        .json(&quench_serde::json!({ "refresh_token": refresh_token }))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let tokens: TokenResponse = response.json().await.ok()?;
    Some(RefreshedTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    })
}

#[cfg_attr(not(feature = "http"), allow(dead_code))] // only the http layer drives the sign-in flow
pub(crate) fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    quench_core::random::fill(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}
