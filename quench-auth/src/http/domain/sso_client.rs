//! The quench-http half of the relying-party authorization-code + PKCE
//! flow. See `crate::actix::domain::sso_client` for the actix-web
//! equivalent and `crate::domain::sso_client` for the framework-agnostic
//! config/refresh logic both share - this module only differs from the
//! actix one in how it reads the request and builds the response.

use crate::domain::realm;
use crate::domain::sso_client::{AuthorizeState, TokenResponse, random_urlsafe};
use crate::http::domain::cookies::cookie_value;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Request, Response};
use sha2::{Digest, Sha256};

pub use crate::domain::sso_client::{RefreshedTokens, SsoConfig, refresh};

/// Redirects the browser to gatehouse's `/authorize`, starting the code
/// exchange. See `crate::actix::domain::sso_client::authorize_redirect`.
pub fn authorize_redirect(request: &Request, config: &SsoConfig) -> Response {
    let Some(base) = realm::gatehouse_url() else {
        return gatehouse_not_configured();
    };
    if !config.configured() {
        tracing::error!(
            "GATEHOUSE_CLIENT_ID / GATEHOUSE_CLIENT_SECRET not set: this service cannot start \
             the sign-in redirect"
        );
        return gatehouse_not_configured();
    }

    let destination = realm::ui_path("/home");
    let callback_url = absolute_url(request, &realm::ui_path("/auth/callback"));
    let state = random_urlsafe(24);
    let code_verifier = random_urlsafe(48);
    let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));

    let cookie_value = serde_json::to_string(&AuthorizeState {
        state: state.clone(),
        code_verifier,
        redirect: destination,
    })
    .unwrap_or_default();

    let url = format!(
        "{base}/api/v1/authorize?client_id={client_id}&redirect_uri={redirect_uri}&state={state}&code_challenge={code_challenge}&code_challenge_method=S256",
        client_id = urlencoding::encode(&config.client_id),
        redirect_uri = urlencoding::encode(&callback_url),
        state = urlencoding::encode(&state),
        code_challenge = urlencoding::encode(&code_challenge),
    );

    Response::new(StatusCode::FOUND)
        .header("Location", url)
        .append_header(
            "set-cookie",
            realm::authorize_state_cookie(cookie_value).to_string(),
        )
}

/// `GET /ui/auth/callback?code=...&state=...`: exchanges the code for this
/// service's own token pair and sets its local session cookies.
pub async fn callback(request: &Request, config: &SsoConfig) -> Response {
    let Some(base) = realm::gatehouse_url() else {
        return gatehouse_not_configured();
    };

    let Some(cookie) = cookie_value(request, realm::AUTHORIZE_STATE_COOKIE) else {
        return callback_failed("missing authorize state");
    };
    let Ok(saved) = serde_json::from_str::<AuthorizeState>(&cookie) else {
        return callback_failed("corrupt authorize state");
    };

    let query = query_params(request);
    let (Some(code), Some(state)) = (query.get("code"), query.get("state")) else {
        return callback_failed("gatehouse did not return a code");
    };
    if state != &saved.state {
        return callback_failed("state mismatch");
    }

    let callback_url = absolute_url(request, &realm::ui_path("/auth/callback"));
    let http = match reqwest::Client::builder()
        .danger_accept_invalid_certs(!config.tls_verify)
        .build()
    {
        Ok(client) => client,
        Err(_) => return callback_failed("failed to build the token exchange client"),
    };

    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", callback_url.as_str()),
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("code_verifier", saved.code_verifier.as_str()),
    ];

    let response = http
        .post(format!("{base}/api/v1/token"))
        .form(&form)
        .send()
        .await;
    let Ok(response) = response else {
        return callback_failed("could not reach gatehouse");
    };
    if !response.status().is_success() {
        return callback_failed("gatehouse rejected the code exchange");
    }
    let Ok(tokens) = response.json::<TokenResponse>().await else {
        return callback_failed("gatehouse returned an unreadable token response");
    };

    Response::new(StatusCode::FOUND)
        .header("Location", saved.redirect)
        .append_header(
            "set-cookie",
            realm::session_cookie(tokens.access_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::refresh_cookie(tokens.refresh_token).to_string(),
        )
        .append_header(
            "set-cookie",
            realm::cleared_authorize_state_cookie().to_string(),
        )
}

fn callback_failed(reason: &str) -> Response {
    tracing::warn!("sso callback failed: {reason}");
    Response::new(StatusCode::FOUND)
        .header("Location", realm::ui_path("/login"))
        .append_header(
            "set-cookie",
            realm::cleared_authorize_state_cookie().to_string(),
        )
}

fn gatehouse_not_configured() -> Response {
    tracing::error!(
        "GATEHOUSE_URL is not set: this service cannot sign anyone in, because gatehouse owns \
         the login form and the realm session"
    );
    Response::text(
        StatusCode::SERVICE_UNAVAILABLE,
        "gatehouse is not configured",
    )
}

/// Best-effort absolute URL for `path` on this service. Reads
/// `X-Forwarded-Proto`/`X-Forwarded-Host` when present (set by the reverse
/// proxy in front of a deployment), falling back to `https` and the `Host`
/// header - simpler than actix-web's `connection_info()`, which also
/// understands the legacy `Forwarded` header; add that here if a deployment
/// ever needs it.
fn absolute_url(request: &Request, path: &str) -> String {
    let scheme = request.header("x-forwarded-proto").unwrap_or("https");
    let host = request
        .header("x-forwarded-host")
        .or_else(|| request.header("host"))
        .unwrap_or("");
    format!("{scheme}://{host}{path}")
}

fn query_params(request: &Request) -> std::collections::HashMap<String, String> {
    request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            let value = urlencoding::decode(value).ok()?.into_owned();
            Some((key.to_string(), value))
        })
        .collect()
}
