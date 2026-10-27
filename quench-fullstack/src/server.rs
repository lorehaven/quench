//! The server half of server functions: the registry `#[server]` fills, the checks every call
//! passes, and turning errors into responses.

use std::future::Future;
use std::sync::Arc;

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_core::async_trait;
use quench_core::registry::Registry;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Endpoint, FromRequest, HttpError, Request, Response};
use quench_serde::{Deserialize, Serialize, json};

use crate::error::{INTERNAL_MESSAGE, ServerError};
use crate::user::User;
use crate::wire::{CONTENT_TYPE, MAX_BODY, RPC_HEADER, RPC_VALUE};

/// A server function: where it answers and what answers.
#[derive(Debug)]
pub struct ServerFn {
    pub path: &'static str,
    pub endpoint: fn() -> Arc<dyn Endpoint>,
}

/// Every `#[server]` in the linked binary registers itself here before `main`.
///
/// As for routes, a crate whose code the binary never references contributes nothing: make sure
/// the binary uses something from the crate that defines its server functions.
pub static FUNCTIONS: Registry<ServerFn> = Registry::new();

/// The registered functions as `(path, endpoint)`, in registration order.
///
/// # Panics
/// If two functions have the same path, since one would silently shadow the other. Give one a
/// different name or `#[server(path = "...")]`.
#[must_use]
pub fn functions() -> Vec<(&'static str, Arc<dyn Endpoint>)> {
    let mut found = collect(&FUNCTIONS);
    found.reverse(); // the registry iterates newest first
    found
}

/// The `(path, endpoint)` of each function in `items`, in the order given.
///
/// # Panics
/// As for [`functions`].
#[must_use]
pub fn collect<'a>(
    items: impl IntoIterator<Item = &'a ServerFn>,
) -> Vec<(&'static str, Arc<dyn Endpoint>)> {
    let mut found: Vec<(&'static str, Arc<dyn Endpoint>)> = Vec::new();
    for function in items {
        assert!(
            found.iter().all(|(path, _)| *path != function.path),
            "two server functions share the path {}; rename one or give it #[server(path = \"...\")]",
            function.path
        );
        found.push((function.path, (function.endpoint)()));
    }
    found
}

impl ServerError {
    /// The error an extractor or a protocol check failed with.
    #[must_use]
    pub fn from_http(error: &HttpError) -> Self {
        match error {
            HttpError::Status(code, message) => match code.as_u16() {
                401 => Self::Unauthorized(message.clone()),
                403 => Self::Forbidden(message.clone()),
                404 => Self::NotFound(message.clone()),
                400 | 413 | 415 | 422 => Self::Validation {
                    message: message.clone(),
                    fields: Vec::new(),
                },
                _ => Self::Internal(message.clone()),
            },
            HttpError::Extraction(e) => Self::Validation {
                message: e.to_string(),
                fields: Vec::new(),
            },
        }
    }

    /// The response for this error: its status, and the JSON body the browser decodes. An
    /// internal error is logged here and described to the browser only generically.
    #[must_use]
    pub fn into_response(self) -> Response {
        if let Self::Internal(cause) = &self {
            quench_log::error!("server function failed: {cause}");
        }
        let status =
            StatusCode::from_u16(self.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        Response::json(status, &self.to_body())
            .unwrap_or_else(|_| Response::text(StatusCode::INTERNAL_SERVER_ERROR, INTERNAL_MESSAGE))
            .header("cache-control", "no-store")
    }
}

fn refuse(status: StatusCode, error: &ServerError) -> Response {
    Response::json(status, &error.to_body())
        .unwrap_or_else(|_| Response::new(status))
        .header("cache-control", "no-store")
}

/// The host part of an `Origin` or `Host` value: `https://app.example:8443` and `app.example:8443`
/// both give `app.example:8443`.
fn authority(value: &str) -> &str {
    value
        .split_once("://")
        .map_or(value, |(_, rest)| rest)
        .trim_end_matches('/')
}

/// The checks every call passes before anything else runs.
///
/// It must be a `POST` of JSON carrying the [`RPC_HEADER`]. A call that relies on cookies (it has
/// no `Authorization` header) must also come from this origin: `Sec-Fetch-Site`, when the browser
/// sends it, must be `same-origin`, and an `Origin` must be this host. Bearer calls are exempt from
/// the origin checks, since a token in a header is not something a hostile page can attach.
///
/// Returns the response to send instead (405, 415 or 403) when a check fails.
#[must_use]
pub fn guard(req: &Request) -> Option<Response> {
    if req.method().as_str() != "POST" {
        return Some(Response::new(StatusCode::METHOD_NOT_ALLOWED).header("allow", "POST"));
    }
    let json_body = req.header("content-type").is_some_and(|v| {
        v.split(';')
            .next()
            .is_some_and(|t| t.trim().eq_ignore_ascii_case(CONTENT_TYPE))
    });
    if !json_body {
        return Some(Response::new(StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }
    if req.header(RPC_HEADER) != Some(RPC_VALUE) {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            &ServerError::forbidden(format!("missing {RPC_HEADER} header")),
        ));
    }
    if req.header("authorization").is_some() {
        return None;
    }
    if let Some(site) = req.header("sec-fetch-site")
        && site != "same-origin"
        && site != "none"
    {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            &ServerError::forbidden("cross-site request"),
        ));
    }
    if let Some(origin) = req.header("origin")
        && let Some(host) = req.header("host")
        && authority(origin) != authority(host)
    {
        return Some(refuse(
            StatusCode::FORBIDDEN,
            &ServerError::forbidden("cross-origin request"),
        ));
    }
    None
}

/// Reads the arguments, runs `call` and sends its result. What every `#[server]` endpoint does
/// after [`guard`] and the `#[context]` extractors.
pub async fn dispatch<A, T, Fut>(mut req: Request, call: impl FnOnce(A) -> Fut) -> Response
where
    A: Deserialize,
    T: Serialize,
    Fut: Future<Output = Result<T, ServerError>>,
{
    let body = match req.take_body().collect_limited(MAX_BODY).await {
        Ok(body) => body,
        Err(e) => return ServerError::from_http(&HttpError::from(e)).into_response(),
    };
    let args: A = match json::from_slice(&body) {
        Ok(args) => args,
        Err(e) => {
            return ServerError::Validation {
                message: format!("invalid arguments: {e}"),
                fields: Vec::new(),
            }
            .into_response();
        }
    };
    match call(args).await {
        Ok(value) => Response::json(StatusCode::OK, &value)
            .unwrap_or_else(|e| ServerError::internal(e).into_response())
            .header("cache-control", "no-store"),
        Err(error) => error.into_response(),
    }
}

tokio::task_local! {
    static CURRENT_USER: Option<User>;
}

/// Runs `future` with `user` as the answer to [`crate::user`]. What a `#[server(auth)]` endpoint
/// wraps the function in.
pub async fn with_user<T>(user: Option<User>, future: impl Future<Output = T>) -> T {
    CURRENT_USER.scope(user, future).await
}

/// The user of the call being served, for [`crate::user`].
pub(crate) fn current_user() -> Result<User, ServerError> {
    CURRENT_USER
        .try_with(Clone::clone)
        .ok()
        .flatten()
        .ok_or_else(|| ServerError::unauthorized("sign in required"))
}

impl User {
    /// The user a token stands for.
    #[must_use]
    pub fn from_claims(claims: &Claims) -> Self {
        Self {
            sub: claims.sub.clone(),
            scope: claims.scope.clone(),
            audiences: claims.aud.clone(),
            session: claims.sid.clone(),
        }
    }
}

/// A bearer token that verifies, is for this service, and belongs to a live session.
async fn bearer_claims(req: &Request, config: &JwtConfig) -> Option<Claims> {
    let token = req.header("authorization")?.strip_prefix("Bearer ")?;
    let claims = config.decode_claims(token).await.ok()?;
    if !claims.allows(&config.service_name) {
        return None;
    }
    if let (Some(sid), Ok(sessions)) = (claims.sid.as_deref(), req.container().get::<SessionDb>())
        && !sessions.is_active(sid, &claims.sub).await.unwrap_or(false)
    {
        return None;
    }
    Some(claims)
}

#[async_trait]
impl FromRequest for User {
    /// The claims the `Auth` middleware already verified, else a bearer token, else the session
    /// cookie. With `SERVICE_AUTH_ENABLED` off, as in development, everyone is an administrator.
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        if let Some(claims) = req.extensions().get::<Claims>() {
            return Ok(Self::from_claims(claims));
        }
        let config = req.container().get::<JwtConfig>().map_err(|_| {
            HttpError::status(
                StatusCode::INTERNAL_SERVER_ERROR,
                "authentication is not configured",
            )
        })?;
        let claims = match bearer_claims(req, &config).await {
            Some(claims) => Some(claims),
            None => quench_auth::http::routers::ui::get_user_from_req(req, &config).await,
        };
        claims
            .map(|c| Self::from_claims(&c))
            .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED, "sign in required"))
    }
}
