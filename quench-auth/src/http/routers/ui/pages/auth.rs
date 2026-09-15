//! Sending a browser to gatehouse. Ported from
//! `crate::actix::routers::ui::pages::auth`; see that module's doc comment
//! for what this is for. Uses `crate::domain::realm::ui_path` directly
//! rather than the actix version's private duplicate of the same
//! normalization.

use crate::domain::jwt::JwtConfig;
use crate::domain::realm;
use crate::domain::sso_client::{self, SsoConfig};
use crate::http::domain::cookies::cookie_value;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Request, Response};
use serde::{Deserialize, Serialize};

/// `?err=1` on the login page. Gatehouse renders the error; a relying party
/// only ever passes the parameter through.
#[derive(Deserialize)]
pub struct LoginQuery {
    pub err: Option<String>,
}

/// What a page's session watcher is told about the session it is holding.
#[derive(Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub username: Option<String>,
    pub roles: Vec<String>,
}

impl AuthStatus {
    pub fn anonymous() -> Self {
        Self {
            authenticated: false,
            username: None,
            roles: Vec::new(),
        }
    }
}

/// Answers "is the cookie I am holding still worth anything". Always a
/// `200` - see the actix version's doc comment for why.
pub async fn auth_status(request: &Request, config: &JwtConfig) -> Response {
    if !config.auth_enabled {
        return json(&AuthStatus {
            authenticated: true,
            username: Some("dev".to_string()),
            roles: vec!["admin".to_string()],
        });
    }

    let Some(cookie) = cookie_value(request, &realm::session_cookie_name()) else {
        return json(&AuthStatus::anonymous());
    };

    match config.decode_claims(&cookie).await {
        Ok(claims) => json(&AuthStatus {
            authenticated: true,
            username: Some(claims.sub),
            roles: claims
                .scope
                .split(',')
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .collect(),
        }),
        Err(_) => json(&AuthStatus::anonymous()),
    }
}

/// Exchanges this browser's refresh cookie for a fresh token pair. See the
/// actix version's doc comment for why this proxies through the service
/// rather than letting the browser call gatehouse directly.
pub async fn refresh_delegation(request: &Request) -> Response {
    let Some(refresh_token) = cookie_value(request, &realm::refresh_cookie_name()) else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };

    let Some(tokens) = sso_client::refresh(&refresh_token).await else {
        return Response::new(StatusCode::UNAUTHORIZED);
    };

    json(&AuthStatus {
        authenticated: true,
        username: None,
        roles: Vec::new(),
    })
    .append_header(
        "set-cookie",
        realm::session_cookie(tokens.access_token).to_string(),
    )
    .append_header(
        "set-cookie",
        realm::refresh_cookie(tokens.refresh_token).to_string(),
    )
}

/// Sends the browser to sign in - unless a `forge_refresh` cookie is still
/// good enough to renew, in which case this skips gatehouse's login page
/// entirely.
pub async fn login_delegation(request: &Request, sso: &SsoConfig) -> Response {
    if let Some(refresh_token) = cookie_value(request, &realm::refresh_cookie_name())
        && let Some(tokens) = sso_client::refresh(&refresh_token).await
    {
        return Response::new(StatusCode::FOUND)
            .header("Location", realm::ui_path("/home"))
            .append_header(
                "set-cookie",
                realm::session_cookie(tokens.access_token).to_string(),
            )
            .append_header(
                "set-cookie",
                realm::refresh_cookie(tokens.refresh_token).to_string(),
            );
    }

    crate::http::domain::sso_client::authorize_redirect(request, sso)
}

/// `GET /ui/auth/callback` - completes the exchange `login_delegation` started.
pub async fn auth_callback(request: &Request, sso: &SsoConfig) -> Response {
    crate::http::domain::sso_client::callback(request, sso).await
}

/// Realm-wide logout, which is also gatehouse's to perform.
pub fn logout_delegation(request: &Request) -> Response {
    let return_to = absolute_url(request, &realm::ui_path("/login"));
    match realm::gatehouse_logout_url(Some(&return_to)) {
        Some(url) => Response::new(StatusCode::FOUND).header("Location", url),
        None => gatehouse_not_configured(),
    }
}

fn json<T: serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn gatehouse_not_configured() -> Response {
    tracing::error!(
        "GATEHOUSE_URL is not set: this service cannot sign anyone in, because \
         gatehouse owns the login form and the realm session"
    );
    Response::text(
        StatusCode::SERVICE_UNAVAILABLE,
        "gatehouse is not configured",
    )
}

/// Best-effort absolute URL for `path` on this service - see
/// `crate::http::domain::sso_client`'s own `absolute_url` for the same
/// simplification vs. actix's `connection_info()`.
fn absolute_url(request: &Request, path: &str) -> String {
    let scheme = request.header("x-forwarded-proto").unwrap_or("https");
    let host = request
        .header("x-forwarded-host")
        .or_else(|| request.header("host"))
        .unwrap_or("");
    format!("{scheme}://{host}{path}")
}

/// `?redirect=` target, accepted only as a rooted same-origin path or a prefix
/// listed in `AUTH_REDIRECT_HOSTS` - an open redirect here would be a phishing
/// primitive.
pub fn redirect_target(request: &Request) -> Option<String> {
    let query = request.uri().query().unwrap_or("");
    let raw = query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "redirect").then_some(value)
    })?;
    let decoded = urlencoding::decode(raw).ok()?.into_owned();
    validated_redirect(&decoded)
}

pub fn validated_redirect(target: &str) -> Option<String> {
    let same_origin =
        target.starts_with('/') && !target.starts_with("//") && !target.starts_with("/\\");
    let allowed = same_origin
        || allowed_redirect_hosts()
            .iter()
            .any(|prefix| target.starts_with(prefix));
    allowed.then(|| target.to_string())
}

/// Prefixes a `?redirect=` may point at, from `AUTH_REDIRECT_HOSTS`
/// (comma-separated). Empty means same-origin paths only.
pub fn allowed_redirect_hosts() -> Vec<String> {
    envmnt::get_or("AUTH_REDIRECT_HOSTS", "")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .collect()
}
