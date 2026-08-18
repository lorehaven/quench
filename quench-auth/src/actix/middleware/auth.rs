use crate::actix::domain::jwt::{Claims, JwtConfig};
use crate::actix::domain::realm;
use crate::actix::domain::sso_client;
use actix_web::{
    Error, HttpMessage,
    body::{EitherBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    web,
};
use futures_util::future::{LocalBoxFuture, Ready, ok};
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct Auth {
    config: JwtConfig,
}

impl Auth {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

impl<S, B> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = AuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuthMiddleware {
            service: Rc::new(service),
            config: self.config.clone(),
        })
    }
}

pub struct AuthMiddleware<S> {
    service: Rc<S>,
    config: self::JwtConfig,
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(ctx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if !self.config.auth_enabled {
            let service = self.service.clone();
            return Box::pin(async move {
                let res = service.call(req).await?;
                Ok(res.map_into_left_body())
            });
        }

        // Every caller - human or machine - arrives as a bearer token or the
        // realm session cookie now. There is no more password-bearing branch
        // here: a machine identity gets its token from gatehouse's
        // client_credentials grant the same way a browser gets one from the
        // authorization-code flow, so this middleware only ever verifies.
        let header_token = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::to_string);

        let cookie_token = req
            .cookie(&realm::session_cookie_name())
            .map(|cookie| cookie.value().to_string());

        // Kept separate from `header_token`/`cookie_token` above rather than
        // folded into the initial lookup: it is only ever consulted below,
        // once the access token in hand has turned out to be missing or
        // unusable, never as an alternative source of an access token itself.
        let refresh_token = req
            .cookie(&realm::refresh_cookie_name())
            .map(|cookie| cookie.value().to_string());

        let token = header_token.or(cookie_token);

        let config = self.config.clone();
        let service = self.service.clone();
        Box::pin(async move {
            let claims = match &token {
                Some(token_str) => authenticate(token_str, &config, &req).await,
                None => None,
            };

            // A `forge_session` cookie stops decoding after
            // `ACCESS_TOKEN_TTL_SECS` (15 minutes by default) long before the
            // week-long `forge_refresh` cookie sitting next to it does. Absent
            // this, every relying party bounced the browser straight to
            // gatehouse's login page on that schedule regardless of how live
            // the refresh cookie still was - the session cookie's `Max-Age` is
            // unset (a browser-session cookie), so it lingers, stale, exactly
            // long enough to keep tricking a user into thinking they're still
            // signed in. Silently exchanging it here, the same way
            // `gatehouse-service`'s own `/refresh` cookie-flow does, is what
            // keeps a live refresh token actually useful past the access
            // token's TTL instead of only serving API callers who spend it by
            // hand.
            let (claims, refreshed) = match claims {
                Some(claims) => (claims, None),
                None => {
                    let Some(refresh_token) = refresh_token else {
                        if token.is_none() {
                            tracing::warn!("AuthMiddleware: No token found in header or cookie");
                        }
                        let res = unauthorized(&req).map_into_right_body();
                        return Ok(req.into_response(res));
                    };
                    let Some(tokens) = sso_client::refresh(&refresh_token).await else {
                        let res = unauthorized(&req).map_into_right_body();
                        return Ok(req.into_response(res));
                    };
                    let Some(claims) = authenticate(&tokens.access_token, &config, &req).await
                    else {
                        let res = unauthorized(&req).map_into_right_body();
                        return Ok(req.into_response(res));
                    };
                    (claims, Some(tokens))
                }
            };

            req.extensions_mut().insert(claims);
            let res = service.call(req).await?;
            let mut res = res.map_into_left_body();
            if let Some(tokens) = refreshed {
                let _ = res
                    .response_mut()
                    .add_cookie(&realm::session_cookie(tokens.access_token));
                let _ = res
                    .response_mut()
                    .add_cookie(&realm::refresh_cookie(tokens.refresh_token));
            }
            Ok(res)
        })
    }
}

/// Runs the checks a bearer/cookie access token must pass: decodes, confirms
/// this service is in its audience, and - for tokens carrying a session id -
/// confirms that session is still active. Shared between the first attempt
/// with whatever token the request arrived with and the retry after
/// [`sso_client::refresh`], so the two can't drift into checking different
/// things.
async fn authenticate(token: &str, config: &JwtConfig, req: &ServiceRequest) -> Option<Claims> {
    let claims = match config.decode_claims(token).await {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!("AuthMiddleware: Failed to decode claims: {:?}", e);
            return None;
        }
    };

    if !claims.allows(&config.service_name) {
        tracing::warn!(
            "AuthMiddleware: Token audience mismatch. Expected {}, got {:?}",
            config.service_name,
            claims.aud
        );
        return None;
    }

    if let Some(session_id) = claims.sid.as_deref() {
        let session_db = req
            .app_data::<web::Data<std::sync::Arc<crate::actix::domain::session::SessionDb>>>()
            .cloned();
        let active = match session_db {
            Some(session_db) => session_db
                .is_active(session_id, &claims.sub)
                .await
                .unwrap_or(false),
            None => {
                tracing::warn!("AuthMiddleware: SessionDb not found in app_data");
                false
            }
        };
        if !active {
            tracing::warn!(
                "AuthMiddleware: Session {} is not active for user {}",
                session_id,
                claims.sub
            );
            return None;
        }
    }

    Some(claims)
}

/// 401 for API callers; a redirect to the gatehouse login for browsers, so an
/// expired session lands on the realm login page instead of a blank error.
fn unauthorized(req: &ServiceRequest) -> actix_web::HttpResponse {
    if wants_html(req)
        && let Some(login_url) =
            crate::actix::domain::realm::gatehouse_login_url(Some(&req.uri().to_string()))
    {
        return actix_web::HttpResponse::Found()
            .append_header(("Location", login_url))
            .finish();
    }

    actix_web::HttpResponse::Unauthorized()
        .append_header(("WWW-Authenticate", "Bearer"))
        .finish()
}

fn wants_html(req: &ServiceRequest) -> bool {
    req.headers()
        .get("Accept")
        .and_then(|accept| accept.to_str().ok())
        .is_some_and(|accept| accept.contains("text/html"))
}
