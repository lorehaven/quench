use crate::error::HttpError;
use crate::request::Request;
use crate::response::Response;
use async_trait::async_trait;

/// The unit of request handling - poem/tower-flavored (a plain async
/// `call`), not actix's `Service`/`Transform` pair. `#[get]`/`#[post]`/...
/// expands a handler function into one of these; middleware wraps one
/// `Endpoint` in another.
#[async_trait]
pub trait Endpoint: Send + Sync {
    async fn call(&self, req: Request) -> Response;
}

/// Converts a handler's return value into a [`Response`]. Implemented for
/// `Response` itself, common scalar bodies, and `Result<T, E>` where both
/// sides implement it - so a handler can just return `Json<T>`,
/// `Result<Json<T>, HttpError>`, `&'static str`, etc.
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Response {
        Response::from(())
    }
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::from(self)
    }
}

impl IntoResponse for &'static str {
    fn into_response(self) -> Response {
        Response::from(self)
    }
}

impl IntoResponse for crate::error::HttpError {
    fn into_response(self) -> Response {
        HttpError::into_response(self)
    }
}

impl<T> IntoResponse for crate::extract::Json<T>
where
    T: serde::Serialize,
{
    fn into_response(self) -> Response {
        match Response::json(http::StatusCode::OK, &self.0) {
            Ok(resp) => resp,
            Err(e) => Response::text(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    }
}

impl<T, E> IntoResponse for Result<T, E>
where
    T: IntoResponse,
    E: IntoResponse,
{
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(err) => err.into_response(),
        }
    }
}
