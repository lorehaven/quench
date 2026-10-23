//! The `{"error": "..."}` shape most of the estate's JSON APIs answer with,
//! plus a small `ApiError` a service's own error type can convert into.

use quench_http::prelude::Response;
use quench_http::prelude::http::StatusCode;
use quench_serde::json;

#[must_use]
pub fn json_error(status: StatusCode, message: &str) -> Response {
    Response::json(status, &json!({ "error": message }))
        .unwrap_or_else(|_| Response::text(status, message.to_string()))
}

/// A status + message a handler can build with `?` once its own error type
/// has a `From` impl into this.
#[derive(Debug)]
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
    #[must_use]
    pub fn into_response(self) -> Response {
        if self.status.is_server_error() {
            quench_log::error!("api error: {}", self.message);
        }
        json_error(self.status, &self.message)
    }
}

impl quench_http::prelude::IntoResponse for ApiError {
    fn into_response(self) -> Response {
        Self::into_response(self)
    }
}
