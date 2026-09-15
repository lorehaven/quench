pub mod pages;

use crate::domain::jwt::{Claims, JwtConfig};
use crate::domain::realm;
use crate::domain::session::SessionDb;
use crate::http::domain::cookies::cookie_value;
use quench_http::prelude::Request;

/// Whether the request carries a usable realm session.
///
/// Checks the session store as well as the token: identity is shared now, so a
/// logout at any service must take effect everywhere immediately rather than
/// when the access token happens to expire.
pub async fn is_ui_authenticated(req: &Request, config: &JwtConfig) -> bool {
    if !config.auth_enabled {
        return true;
    }

    let Some(cookie) = cookie_value(req, &realm::session_cookie_name()) else {
        return false;
    };

    let Ok(claims) = config.decode_claims(&cookie).await else {
        return false;
    };
    if !claims.allows(&config.service_name) {
        return false;
    }

    session_is_active(req, &claims).await
}

/// Looks up the claim's session via whichever `SessionDb` the service
/// registered in its DI container. Tokens without a `sid`
/// (machine-to-machine) carry no session to check.
async fn session_is_active(req: &Request, claims: &Claims) -> bool {
    let Some(session_id) = claims.sid.as_deref() else {
        return true;
    };

    match req.container().get::<SessionDb>() {
        Ok(sessions) => sessions
            .is_active(session_id, &claims.sub)
            .await
            .unwrap_or(false),
        Err(_) => {
            tracing::debug!("no SessionDb registered; trusting token validity alone");
            true
        }
    }
}

pub async fn get_user_from_req(req: &Request, config: &JwtConfig) -> Option<Claims> {
    if let Some(claims) = req.extensions().get::<Claims>() {
        return Some(claims.clone());
    }

    if !config.auth_enabled {
        return Some(Claims::for_audiences(
            "admin".to_string(),
            vec![config.service_name.clone()],
            "admin".to_string(),
            None,
            3600,
        ));
    }

    let cookie = cookie_value(req, &realm::session_cookie_name())?;

    let claims = match config.decode_claims(&cookie).await {
        Ok(c) if c.allows(&config.service_name) => c,
        _ => return None,
    };

    if !session_is_active(req, &claims).await {
        return None;
    }

    Some(claims)
}
