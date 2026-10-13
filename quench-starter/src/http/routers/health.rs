//! Three endpoints: `/health` for "process is up", `/live`/`/ready` for
//! k8s probes, backed by `HealthState` (in `common::health`). There is no
//! trailing-slash normalization - neither probe relies on it.

pub use crate::common::health::HealthState;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Inject, Response, get};
use serde_json::json;

#[get("/health")]
pub async fn health() -> Response {
    Response::text(StatusCode::OK, "OK")
}

#[get("/health/live")]
pub async fn live(Inject(state): Inject<HealthState>) -> Response {
    if state.is_live() {
        Response::json(StatusCode::OK, &json!({ "status": "live" })).unwrap()
    } else {
        Response::json(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({ "status": "not_live" }),
        )
        .unwrap()
    }
}

#[get("/health/ready")]
pub async fn ready(Inject(state): Inject<HealthState>) -> Response {
    if state.is_ready() {
        Response::json(StatusCode::OK, &json!({ "status": "ready" })).unwrap()
    } else {
        Response::json(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({ "status": "not_ready" }),
        )
        .unwrap()
    }
}

/// See `swagger::register_routes`'s doc comment - same reason.
pub(crate) fn register_routes() {
    let _ = health as fn() -> _;
    let _ = live as fn(Inject<HealthState>) -> _;
    let _ = ready as fn(Inject<HealthState>) -> _;
}
