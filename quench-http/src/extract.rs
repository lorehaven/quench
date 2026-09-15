use crate::error::{ExtractError, HttpError};
use crate::request::Request;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use std::any::Any;
use std::ops::Deref;
use std::sync::Arc;

/// Pulls typed data out of an incoming [`Request`]. Implemented for
/// [`Path`], [`Query`], [`Json`], [`Inject`], and raw body accessors; a
/// `#[get]`/`#[post]`/... handler's parameters are each resolved through
/// this trait, in order, before the handler body runs.
#[async_trait]
pub trait FromRequest: Sized {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError>;
}

/// Deserializes captured path parameters (e.g. `/{name:.+}/blobs/{digest}`)
/// into `T`.
pub struct Path<T>(pub T);

impl<T> Deref for Path<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send> FromRequest for Path<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        req.params
            .load()
            .map(Path)
            .map_err(|e| ExtractError::InvalidPath(e.to_string()).into())
    }
}

/// Deserializes the request's query string into `T`.
pub struct Query<T>(pub T);

impl<T> Deref for Query<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send> FromRequest for Query<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let query = req.uri().query().unwrap_or("");
        serde_urlencoded::from_str(query)
            .map(Query)
            .map_err(|e| ExtractError::InvalidQuery(e.to_string()).into())
    }
}

/// The cap a buffered-body extractor (`Json<T>`, `Bytes`) falls back to
/// when nothing more specific was configured for this request - either no
/// [`BodyLimit`] was provided at all, or (for `Json`/`Bytes`) the handler
/// didn't ask for a different one via [`Limited`].
pub const DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024;

/// An app-wide override for [`DEFAULT_BODY_LIMIT`], seeded once via
/// `ContainerBuilder::provide(BodyLimit(n))` - the quench-http equivalent
/// of actix-web's `web::PayloadConfig::new(n)`. A service that accepts
/// large uploads (warehouse's blobs/files, sized well past a JSON
/// payload's usual few KB) provides one bigger limit that every buffered
/// extractor in the app then respects, rather than raising a limit that
/// was never configurable per call site. Use [`Limited`] instead when only
/// *one* route needs a different limit than the rest of the app.
#[derive(Debug, Clone, Copy)]
pub struct BodyLimit(pub usize);

impl Default for BodyLimit {
    fn default() -> Self {
        Self(DEFAULT_BODY_LIMIT)
    }
}

pub(crate) fn resolve_body_limit(req: &Request) -> usize {
    req.container()
        .get::<BodyLimit>()
        .map(|limit| limit.0)
        .unwrap_or(DEFAULT_BODY_LIMIT)
}

/// Buffers and deserializes a JSON request body into `T`, up to the
/// request's [`BodyLimit`] (falling back to [`DEFAULT_BODY_LIMIT`] if none
/// was provided).
pub struct Json<T>(pub T);

impl<T> Deref for Json<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send> FromRequest for Json<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let limit = resolve_body_limit(req);
        let bytes = req.take_body().collect_limited(limit).await?;
        serde_json::from_slice(&bytes)
            .map(Json)
            .map_err(|e| ExtractError::InvalidJson(e.to_string()).into())
    }
}

/// Buffers and deserializes an `application/x-www-form-urlencoded` request
/// body into `T` - the body-reading counterpart to [`Query`], for an HTML
/// `<form method="post">` (no JS/htmx) rather than a `?query=string`. Same
/// [`BodyLimit`] as `Json`/`Bytes`.
pub struct Form<T>(pub T);

impl<T> Deref for Form<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send> FromRequest for Form<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let limit = resolve_body_limit(req);
        let bytes = req.take_body().collect_limited(limit).await?;
        serde_urlencoded::from_bytes(&bytes)
            .map(Form)
            .map_err(|e| ExtractError::InvalidForm(e.to_string()).into())
    }
}

/// Buffers the raw request body, up to the request's [`BodyLimit`] - the
/// non-JSON counterpart to [`Json`], for a handler that wants the bytes
/// themselves (a non-JSON upload small enough to buffer, a signature to
/// verify over the raw body, ...). A body large enough to want streaming
/// instead should go through [`crate::body::InboundBody`] via
/// `Request::take_body` directly, uncapped.
pub struct Bytes(pub bytes::Bytes);

impl Deref for Bytes {
    type Target = bytes::Bytes;
    fn deref(&self) -> &bytes::Bytes {
        &self.0
    }
}

#[async_trait]
impl FromRequest for Bytes {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let limit = resolve_body_limit(req);
        req.take_body()
            .collect_limited(limit)
            .await
            .map(Bytes)
            .map_err(Into::into)
    }
}

/// Wraps another extractor (`Json<T>`, `Bytes`) to use `LIMIT` bytes
/// instead of the request's [`BodyLimit`] - for the one route that needs a
/// different cap than the rest of the app, without having to change what
/// every other buffered extractor falls back to:
///
/// ```ignore
/// async fn handler(Limited::<Json<Report>, 10_000_000>(Json(report)): Limited<Json<Report>, 10_000_000>) { ... }
/// ```
pub struct Limited<T, const LIMIT: usize>(pub T);

impl<T, const LIMIT: usize> Deref for Limited<T, LIMIT> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: DeserializeOwned + Send, const LIMIT: usize> FromRequest for Limited<Json<T>, LIMIT> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let bytes = req.take_body().collect_limited(LIMIT).await?;
        serde_json::from_slice(&bytes)
            .map(|value| Limited(Json(value)))
            .map_err(|e| ExtractError::InvalidJson(e.to_string()).into())
    }
}

#[async_trait]
impl<const LIMIT: usize> FromRequest for Limited<Bytes, LIMIT> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        req.take_body()
            .collect_limited(LIMIT)
            .await
            .map(|b| Limited(Bytes(b)))
            .map_err(Into::into)
    }
}

/// Reads a value a middleware stashed on this request via
/// `req.extensions_mut().insert(value)` - e.g. auth middleware inserting
/// decoded claims for a handler to read back. Distinct from [`Inject`]:
/// this is per-request *data*, not a shared *service* the DI container
/// resolves once.
pub struct Extension<T>(pub T);

impl<T> Deref for Extension<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> FromRequest for Extension<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        req.extensions()
            .get::<T>()
            .cloned()
            .map(Extension)
            .ok_or_else(|| {
                HttpError::status(
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "`{}` was not set on this request",
                        std::any::type_name::<T>()
                    ),
                )
            })
    }
}

/// Resolves a dependency-injected service (anything `#[injectable]`, or
/// seeded via `ContainerBuilder::provide`) straight out of the request's
/// container.
pub struct Inject<T: ?Sized>(pub Arc<T>);

impl<T: ?Sized> Deref for Inject<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> Clone for Inject<T> {
    fn clone(&self) -> Self {
        Inject(self.0.clone())
    }
}

#[async_trait]
impl<T: Any + Send + Sync + 'static> FromRequest for Inject<T> {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        req.container()
            .get::<T>()
            .map(Inject)
            .map_err(|e| HttpError::status(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
    }
}
