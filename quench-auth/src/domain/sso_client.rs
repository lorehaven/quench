//! The framework-agnostic half of the relying-party side of the
//! authorization-code + PKCE flow: config, the token-refresh HTTP call, and
//! the PKCE state shape. The parts that build a redirect response or read
//! the incoming request (`authorize_redirect`, `callback`, ...) stay
//! per-framework - see `crate::actix::domain::sso_client` and
//! `crate::http::domain::sso_client` - since they're the only parts that
//! actually touch `HttpRequest`/`HttpResponse` (or quench-http's
//! equivalents).

use crate::domain::realm;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct SsoConfig {
    pub client_id: String,
    pub(crate) client_secret: String,
    pub(crate) tls_verify: bool,
}

impl SsoConfig {
    pub fn init() -> Self {
        let client_id = envmnt::get_or("GATEHOUSE_CLIENT_ID", "");
        let client_secret = envmnt::get_or("GATEHOUSE_CLIENT_SECRET", "");
        let tls_verify = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
            .parse()
            .unwrap_or(true);
        Self {
            client_id,
            client_secret,
            tls_verify,
        }
    }

    pub(crate) fn configured(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && realm::gatehouse_url().is_some()
    }
}

#[derive(Serialize, Deserialize)]
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

/// Trades a `forge_refresh` value for a fresh pair via gatehouse's `/refresh`;
/// `None` on any failure, so the caller just falls back to a full sign-in.
pub async fn refresh(refresh_token: &str) -> Option<RefreshedTokens> {
    let base = realm::gatehouse_url()?;
    let tls_verify: bool = envmnt::get_or("GATEHOUSE_TLS_VERIFY", "true")
        .parse()
        .unwrap_or(true);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(!tls_verify)
        .build()
        .ok()?;
    let response = client
        .post(format!("{base}/api/v1/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
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

pub(crate) fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}
