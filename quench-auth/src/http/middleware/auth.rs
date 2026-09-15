//! Ported from `crate::actix::middleware::auth`. Same rules: bearer header
//! or realm session cookie, with a silent refresh-token exchange when the
//! access token is gone but the refresh token isn't; same unauthorized
//! behavior (redirect for a browser, 401 for an API caller). The one real
//! difference is where `Claims` lands for a handler to read: actix stashed
//! them in `ServiceRequest::extensions_mut()`; here it's
//! `Request::extensions_mut()` - see `crate::http`'s module doc for why
//! quench-http needed that added at all.
//!
//! `SessionDb` comes from the DI container (`req.container().get::<SessionDb>()`)
//! rather than `app_data`, which is the quench-http equivalent - a service
//! wires it in once via `ContainerBuilder::provide_arc`.

use crate::domain::jwt::{Claims, JwtConfig};
use crate::domain::realm;
use crate::domain::session::SessionDb;
use crate::domain::sso_client;
use crate::http::domain::cookies::cookie_value;
use async_trait::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

pub struct Auth {
    config: JwtConfig,
}

impl Auth {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Middleware for Auth {
    async fn handle(&self, mut req: Request, next: &dyn Endpoint) -> Response {
        if !self.config.auth_enabled {
            return next.call(req).await;
        }

        // Every caller - human or machine - arrives as a bearer token or the
        // realm session cookie now. There is no more password-bearing branch
        // here: a machine identity gets its token from gatehouse's
        // client_credentials grant the same way a browser gets one from the
        // authorization-code flow, so this middleware only ever verifies.
        let header_token = req
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);
        let cookie_token = cookie_value(&req, &realm::session_cookie_name());
        // Only consulted below, as a fallback once `token` turns out unusable.
        let refresh_token = cookie_value(&req, &realm::refresh_cookie_name());
        let token = header_token.or(cookie_token);

        let claims = match &token {
            Some(token_str) => authenticate(token_str, &self.config, &req).await,
            None => None,
        };

        // The access token outlives its `ACCESS_TOKEN_TTL_SECS` far less than
        // the refresh token does - silently exchange it here instead of
        // bouncing a still-valid session to login.
        let (claims, refreshed) = match claims {
            Some(claims) => (claims, None),
            None => {
                let Some(refresh_token) = refresh_token else {
                    if token.is_none() {
                        tracing::warn!("Auth: no token found in header or cookie");
                    }
                    return unauthorized(&req);
                };
                let Some(tokens) = sso_client::refresh(&refresh_token).await else {
                    return unauthorized(&req);
                };
                let Some(claims) = authenticate(&tokens.access_token, &self.config, &req).await
                else {
                    return unauthorized(&req);
                };
                (claims, Some(tokens))
            }
        };

        req.extensions_mut().insert(claims);
        let response = next.call(req).await;

        match refreshed {
            Some(tokens) => response
                .append_header(
                    "set-cookie",
                    realm::session_cookie(tokens.access_token).to_string(),
                )
                .append_header(
                    "set-cookie",
                    realm::refresh_cookie(tokens.refresh_token).to_string(),
                ),
            None => response,
        }
    }
}

/// Decodes and validates one access token. Shared by the first attempt and
/// the retry after [`sso_client::refresh`], so both check the same things.
async fn authenticate(token: &str, config: &JwtConfig, req: &Request) -> Option<Claims> {
    let claims = match config.decode_claims(token).await {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!("Auth: failed to decode claims: {e:?}");
            return None;
        }
    };

    if !claims.allows(&config.service_name) {
        tracing::warn!(
            "Auth: token audience mismatch. Expected {}, got {:?}",
            config.service_name,
            claims.aud
        );
        return None;
    }

    if let Some(session_id) = claims.sid.as_deref() {
        let active = match req.container().get::<SessionDb>() {
            Ok(session_db) => session_db
                .is_active(session_id, &claims.sub)
                .await
                .unwrap_or(false),
            Err(_) => {
                tracing::warn!("Auth: SessionDb not found in the container");
                false
            }
        };
        if !active {
            tracing::warn!(
                "Auth: session {session_id} is not active for user {}",
                claims.sub
            );
            return None;
        }
    }

    Some(claims)
}

/// 401 for API callers; a redirect to the gatehouse login for browsers, so an
/// expired session lands on the realm login page instead of a blank error.
fn unauthorized(req: &Request) -> Response {
    if wants_html(req)
        && let Some(login_url) = realm::gatehouse_login_url(Some(&req.uri().to_string()))
    {
        return Response::new(StatusCode::FOUND).header("Location", login_url);
    }

    Response::new(StatusCode::UNAUTHORIZED).header("WWW-Authenticate", "Bearer")
}

fn wants_html(req: &Request) -> bool {
    req.header("accept")
        .is_some_and(|accept| accept.contains("text/html"))
}
