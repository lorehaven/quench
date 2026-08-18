//! The `{"error": "..."}` shape most of the estate's JSON APIs answer with,
//! plus a small `ApiError` a service's own error type can convert into.

use actix_web::{HttpResponse, http::StatusCode};
use serde_json::json;

pub fn json_error(status: StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message }))
}

/// A status + message a handler can build with `?` once its own error type
/// has a `From` impl into this.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    /// Logs on the way out for a 5xx, so a server error doesn't disappear
    /// silently into a JSON body nobody reads.
    pub fn into_response(self) -> HttpResponse {
        if self.status.is_server_error() {
            tracing::error!("api error: {}", self.message);
        }
        json_error(self.status, &self.message)
    }
}
