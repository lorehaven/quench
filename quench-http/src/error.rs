use crate::response::Response;
use std::fmt;

/// Error produced while resolving the dependency graph.
#[derive(Debug, thiserror::Error)]
pub enum DiError {
    #[error(
        "component `{component}` depends on `{dependency}`, which is not registered (no #[injectable] or Container::provide for it)"
    )]
    MissingDependency {
        component: &'static str,
        dependency: &'static str,
    },
    #[error("dependency cycle detected among: {}", .0.join(" -> "))]
    Cycle(Vec<&'static str>),
    #[error("constructing `{component}` failed: {reason}")]
    ConstructionFailed {
        component: &'static str,
        reason: String,
    },
    #[error("`{0}` was requested from the container but is not registered")]
    NotFound(&'static str),
}

impl DiError {
    pub fn not_found<T: 'static>() -> Self {
        Self::NotFound(std::any::type_name::<T>())
    }
}

/// Error that can be turned into an HTTP response by an endpoint.
pub trait IntoHttpError: fmt::Debug + Send + Sync + 'static {
    fn into_response(self: Box<Self>) -> Response;
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("{0}")]
    Status(http::StatusCode, String),
    #[error(transparent)]
    Extraction(#[from] ExtractError),
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("missing path parameter `{0}`")]
    MissingPathParam(&'static str),
    #[error("invalid path parameter: {0}")]
    InvalidPath(String),
    #[error("invalid query string: {0}")]
    InvalidQuery(String),
    #[error("invalid JSON body: {0}")]
    InvalidJson(String),
    #[error("invalid form body: {0}")]
    InvalidForm(String),
    #[error("invalid multipart body: {0}")]
    InvalidMultipart(String),
    #[error("body exceeds the {0} byte limit")]
    BodyTooLarge(usize),
    #[error("failed to read request body: {0}")]
    BodyReadFailed(String),
    #[error("`{0}` is not available in application state")]
    MissingState(&'static str),
}

impl HttpError {
    pub fn status(code: http::StatusCode, msg: impl Into<String>) -> Self {
        Self::Status(code, msg.into())
    }

    pub fn into_response(self) -> Response {
        let status = match &self {
            HttpError::Status(code, _) => *code,
            HttpError::Extraction(_) => http::StatusCode::BAD_REQUEST,
        };
        Response::text(status, self.to_string())
    }
}
