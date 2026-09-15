use quench_http::prelude::Response;
use quench_http::prelude::http::StatusCode;
use serde::Serialize;
use serde_json::{Value, json};

pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
pub const DENIED: &str = "DENIED";
pub const UNSUPPORTED: &str = "UNSUPPORTED";

#[derive(Serialize)]
struct AuthErrorBody {
    errors: Vec<AuthErrorEntry>,
}

#[derive(Serialize)]
struct AuthErrorEntry {
    code: &'static str,
    message: &'static str,
    detail: Value,
}

pub fn response(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    response_with_detail(status, code, message, json!({}))
}

pub fn response_with_detail(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    detail: Value,
) -> Response {
    let body = AuthErrorBody {
        errors: vec![AuthErrorEntry {
            code,
            message,
            detail,
        }],
    };
    Response::json(status, &body).unwrap_or_else(|_| Response::text(status, message))
}
